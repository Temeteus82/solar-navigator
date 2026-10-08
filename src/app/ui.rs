use super::camera::toggle_camera_mode_impl;
use super::types::{
    AU_TO_SCENE_UNITS, AppStatus, BODIES, BodyKind, BodyRuntime, BodyTrails, CameraMode,
    HorizonsSyncState, KM_PER_AU, MAX_SIMULATION_RATE_MULTIPLIER, MIN_SIMULATION_RATE_MULTIPLIER,
    OrbitCameraState, RenderSettings, SECONDS_PER_DAY, SIDE_PANEL_WIDTH_PX, SimulationEpoch,
    SimulationState, TextureStatus,
};
use super::util::format_simulation_speed;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use chrono::{Datelike, Duration as ChronoDuration, NaiveDate};
use egui_phosphor::fill as icon;

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_side_panel(
    mut contexts: EguiContexts,
    time: Res<Time>,
    app_status: Res<AppStatus>,
    mut horizons_sync: ResMut<HorizonsSyncState>,
    mut simulation_state: ResMut<SimulationState>,
    mut render_settings: ResMut<RenderSettings>,
    mut orbit_camera: ResMut<OrbitCameraState>,
    texture_status: Res<TextureStatus>,
    simulation_epoch: Res<SimulationEpoch>,
    body_runtime: Res<BodyRuntime>,
    mut trails: ResMut<BodyTrails>,
    mut diagnostics_were_flagged: Local<bool>,
) -> Result {
    let mode_text = if app_status.spice_enabled {
        "SPICE"
    } else {
        "Fallback"
    };

    // Fonts, colours and spacing are set once by `theme::apply_theme`.
    let ctx = contexts.ctx_mut()?;
    let theme = ctx.theme();
    // Keep a small right inner_margin for readability but not so wide that
    // the blank gutter makes the separator visible. The separator line itself
    // is suppressed via show_separator_line(false) below.
    let panel_frame = egui::Frame::side_top_panel(&ctx.style_of(theme))
        .stroke(egui::Stroke::NONE)
        .inner_margin(egui::Margin {
            left: 8,
            right: 4,
            top: 2,
            bottom: 2,
        });

    // egui 0.35 unified Side/TopBottom/CentralPanel behind `Panel`, which is
    // shown inside a `Ui` rather than directly against the `Context`. Build a
    // full-viewport root `Ui` to anchor it to, matching bevy_egui's own
    // side_panel example.
    let mut viewport_ui = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("navigator_root_ui"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::left("navigator_side_panel")
        .exact_size(SIDE_PANEL_WIDTH_PX)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame)
        .show(&mut viewport_ui, |ui| {
            ui.heading("Solar Navigator");
            ui.small(format!("Mode: {mode_text}"));
            ui.add_space(4.0);

            // Primary navigation: a dropdown of every body, pinned above the
            // scrolling sections so it is always one click away.
            let target_label = ui.label(format!("{} Target", icon::CROSSHAIR));
            let selected_name = simulation_state
                .selected_body_index
                .and_then(|index| BODIES.get(index))
                .map_or("Select a body…", |body| body.display_name);
            egui::ComboBox::from_id_salt("target_body")
                .selected_text(selected_name)
                .width(ui.available_width())
                // Let the list use the window's height so every group shows
                // without scrolling; egui clamps the popup to the screen. (An
                // infinite height is not honoured — it falls back to ~400 px.)
                .height(ctx.viewport_rect().height())
                .show_ui(ui, |ui| {
                    for (group, kind) in BodyKind::ALL.into_iter().enumerate() {
                        if group > 0 {
                            ui.separator();
                        }
                        ui.label(egui::RichText::new(kind.group_label()).small().strong());
                        for (index, body) in BODIES.iter().enumerate() {
                            if body.kind != kind {
                                continue;
                            }
                            let selected = simulation_state.selected_body_index == Some(index);
                            if ui.selectable_label(selected, body.display_name).clicked() {
                                simulation_state.jump_request = Some(index);
                            }
                        }
                    }
                })
                .response
                // Name the dropdown for screen readers (WCAG 3.3.2 / 4.1.2).
                .labelled_by(target_label.id);
            ui.add_space(6.0);

            // Everything below scrolls together so no section is ever clipped on
            // a short window.
            egui::ScrollArea::vertical()
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    egui::CollapsingHeader::new(format!(
                        "{} Time & simulation",
                        icon::CLOCK_COUNTDOWN
                    ))
                    .default_open(true)
                    .show(ui, |ui| {
                        let paused_text = if simulation_state.paused {
                            format!("{} paused", icon::PAUSE)
                        } else {
                            format!("{} running", icon::PLAY)
                        };
                        let elapsed_days = simulation_state.elapsed_simulation_days;
                        let current_utc = simulation_epoch.start_utc
                            + ChronoDuration::milliseconds((elapsed_days * 86_400_000.0) as i64);
                        ui.label(format!(
                            "Date: {}",
                            current_utc.format("%Y-%m-%d %H:%M:%S UTC")
                        ));
                        ui.small(format!("Elapsed: {elapsed_days:+.3} days from launch"));
                        ui.label(format!(
                            "Sim: {paused_text} | Speed: {}",
                            format_simulation_speed(simulation_state.simulation_rate)
                        ));
                        ui.small(format!(
                            "Days/s equivalent: {:.7}",
                            simulation_state.simulation_rate / SECONDS_PER_DAY
                        ));
                        ui.add(
                            egui::Slider::new(
                                &mut simulation_state.simulation_rate,
                                MIN_SIMULATION_RATE_MULTIPLIER..=MAX_SIMULATION_RATE_MULTIPLIER,
                            )
                            .logarithmic(true)
                            .text("x realtime"),
                        );

                        ui.add_space(4.0);
                        ui.label("Jump to date:");
                        let max_day = days_in_month(
                            simulation_state.picker_year,
                            simulation_state.picker_month,
                        );
                        simulation_state.picker_day = simulation_state.picker_day.clamp(1, max_day);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut simulation_state.picker_year)
                                    .range(1600..=2200)
                                    .prefix("Y "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut simulation_state.picker_month)
                                    .range(1..=12)
                                    .prefix("M "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut simulation_state.picker_day)
                                    .range(1..=max_day)
                                    .prefix("D "),
                            );
                        });
                        if ui
                            .button(format!("{} Go to Date", icon::CALENDAR_CHECK))
                            .clicked()
                            && let Some(date) = NaiveDate::from_ymd_opt(
                                simulation_state.picker_year,
                                simulation_state.picker_month,
                                simulation_state.picker_day,
                            )
                        {
                            let target = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
                            let diff = target.signed_duration_since(simulation_epoch.start_utc);
                            simulation_state.elapsed_simulation_days =
                                diff.num_seconds() as f64 / 86_400.0;
                            trails.clear();
                        }
                    });

                    egui::CollapsingHeader::new(format!("{} Display", icon::EYE))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.small(format!(
                                "Distance scale: 1 AU = {AU_TO_SCENE_UNITS:.1} units (realistic)"
                            ));
                            ui.checkbox(&mut render_settings.stars_enabled, "Starfield backdrop");
                            ui.checkbox(
                                &mut render_settings.atmosphere_enabled,
                                "Atmosphere halos",
                            );
                            ui.checkbox(&mut render_settings.trails_enabled, "Orbital trails");
                            ui.checkbox(&mut render_settings.rings_enabled, "Planetary rings");
                            ui.checkbox(&mut render_settings.orbits_enabled, "Orbital paths");
                            ui.checkbox(&mut render_settings.asteroids_enabled, "Asteroid belt");
                        });

                    // Always render this header so selecting/deselecting a body
                    // swaps its contents in place instead of shifting the layout.
                    egui::CollapsingHeader::new(format!("{} Selected body", icon::PLANET))
                        .default_open(true)
                        .show(ui, |ui| {
                            if let Some(selected_index) = simulation_state.selected_body_index
                                && let Some(spec) = BODIES.get(selected_index)
                            {
                                ui.label(spec.display_name);
                                // Substantive body facts read at body size (15pt)
                                // rather than 11pt `small` for legibility at desk
                                // viewing distance (WCAG 1.4.4 / readability).
                                ui.label(format!(
                                    "Radius: {}",
                                    format_radius(spec.physical_radius_km)
                                ));
                                ui.label(format!("Mass: {:.3e} kg", spec.mass_kg));
                                if let Some(period_days) = spec.orbital_period_days {
                                    if period_days < 800.0 {
                                        ui.label(format!("Orbital period: {period_days:.2} days"));
                                    } else {
                                        ui.label(format!(
                                            "Orbital period: {:.2} years",
                                            period_days / 365.256
                                        ));
                                    }
                                }
                                if let Some(sma_au) = spec.semi_major_axis_au {
                                    ui.label(format!("Semi-major axis: {sma_au:.3} AU"));
                                }
                                if let Some(position) = body_runtime.positions.get(selected_index) {
                                    let distance_au = position.length() / AU_TO_SCENE_UNITS;
                                    let distance_km = distance_au * KM_PER_AU;
                                    ui.label(format!(
                                        "Distance from Sun: {distance_au:.3} AU ({:.3e} km)",
                                        distance_km
                                    ));
                                    // Light-travel time (one-way) from the Sun.
                                    let light_minutes = distance_au * 499.004784 / 60.0;
                                    ui.label(format!("Light from Sun: {light_minutes:.2} min"));
                                }
                            } else {
                                ui.small("Pick a target above to inspect it.");
                            }
                        });

                    egui::CollapsingHeader::new(format!(
                        "{} Camera & controls",
                        icon::VIDEO_CAMERA
                    ))
                    .default_open(false)
                    .show(ui, |ui| {
                        let camera_mode_label = match orbit_camera.mode {
                            CameraMode::Orbit => "Orbit",
                            CameraMode::Free => "Free fly",
                        };
                        ui.label(format!("Camera: {camera_mode_label}"));
                        ui.small(format!("Distance: {:.2}", orbit_camera.distance));
                        if ui
                            .button(format!("{} Toggle free camera (F)", icon::NAVIGATION_ARROW))
                            .clicked()
                        {
                            toggle_camera_mode_impl(
                                &mut orbit_camera,
                                &mut simulation_state,
                                &body_runtime,
                            );
                        }

                        ui.add_space(4.0);
                        if orbit_camera.mode == CameraMode::Free {
                            ui.label("- WASD: move, Q/E: down/up");
                            ui.label("- Drag: look around");
                            ui.label("- Shift: boost speed");
                            ui.label("- F: back to orbit camera");
                        } else {
                            ui.label("- Left or right drag: orbit");
                            ui.label("- Shift + left drag: pan");
                            ui.label("- Mouse wheel / trackpad scroll: zoom");
                            ui.label("- WASD: orbit, Q/E: zoom (keyboard)");
                            ui.label("- F: free camera");
                        }
                        ui.label("- Space: pause/unpause");
                        ui.label("- Up/Down: simulation speed");
                        ui.label("- Backspace: reset time/view");
                    });

                    // Diagnostics are demoted out of prime real estate, but the
                    // section auto-expands when there's something wrong to see.
                    // `default_open` is only honoured the first time the header
                    // is shown (egui then persists the open/closed state), and
                    // sync/texture failures arrive asynchronously *after* the
                    // first frame — so force the section open on the rising edge
                    // of `has_issues`. The user can still collapse it afterwards.
                    let has_issues =
                        !horizons_sync.failures.is_empty() || !texture_status.failed.is_empty();
                    let issues_just_appeared = has_issues && !*diagnostics_were_flagged;
                    *diagnostics_were_flagged = has_issues;
                    let diagnostics_icon = if has_issues {
                        icon::WARNING
                    } else {
                        icon::INFO
                    };
                    // The icon flips with `has_issues`, so pin the id: egui
                    // otherwise derives it from the title text and would forget
                    // the open/closed state whenever the icon changes.
                    egui::CollapsingHeader::new(format!("{diagnostics_icon} Status & diagnostics"))
                        .id_salt("status_and_diagnostics")
                        .default_open(has_issues)
                        .open(issues_just_appeared.then_some(true))
                        .show(ui, |ui| {
                            ui.small(&app_status.status_line);
                            ui.small(&horizons_sync.status_line);
                            let retry_in_progress = horizons_sync.task.is_some();
                            if ui
                                .add_enabled(
                                    !retry_in_progress,
                                    egui::Button::new(format!(
                                        "{} Retry Horizons Sync",
                                        icon::ARROW_CLOCKWISE
                                    )),
                                )
                                .clicked()
                            {
                                horizons_sync.retry_requested = true;
                                horizons_sync.retry_attempt = 0;
                                horizons_sync.next_retry_deadline_seconds = None;
                                horizons_sync.status_line =
                                    "Horizons sync retry requested".to_string();
                            }
                            if retry_in_progress {
                                ui.small("Horizons sync request in progress...");
                            } else if let Some(deadline) = horizons_sync.next_retry_deadline_seconds
                            {
                                let remaining = (deadline - time.elapsed_secs_f64()).max(0.0);
                                ui.small(format!("Automatic retry in {remaining:.1}s"));
                            }
                            for failure in horizons_sync.failures.iter().take(3) {
                                ui.small(format!("Horizons sync issue: {failure}"));
                            }
                            if horizons_sync.failures.len() > 3 {
                                ui.small(format!(
                                    "Horizons sync issue: ... and {} more",
                                    horizons_sync.failures.len() - 3
                                ));
                            }
                            ui.small(&texture_status.summary);
                            for failure in &texture_status.failed {
                                ui.small(format!("Texture load failed: {failure}"));
                            }
                        });
                });
        });

    Ok(())
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(31)
}

/// Radius with a unit that suits its size: metres below a kilometre (Voyager's
/// 14 m bounding radius would otherwise read "0.01 km"), kilometres above.
fn format_radius(radius_km: f64) -> String {
    if radius_km < 1.0 {
        format!("{:.0} m", radius_km * 1_000.0)
    } else {
        format!("{} km", format_large(radius_km))
    }
}

fn format_large(value: f64) -> String {
    if value >= 10_000.0 {
        format!("{value:.0}")
    } else if value >= 100.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::format_radius;

    #[test]
    fn format_radius_uses_metres_below_a_kilometre() {
        assert_eq!(format_radius(0.014), "14 m");
        assert_eq!(format_radius(0.5), "500 m");
    }

    #[test]
    fn format_radius_keeps_kilometres_for_bodies() {
        assert_eq!(format_radius(1.0), "1.00 km");
        assert_eq!(format_radius(6_371.0), "6371.0 km");
        assert_eq!(format_radius(696_000.0), "696000 km");
    }
}
