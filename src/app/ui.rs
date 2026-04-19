use super::types::{
    AppStatus, BODIES, HorizonsSyncState, MAX_SIMULATION_RATE_MULTIPLIER,
    MIN_SIMULATION_RATE_MULTIPLIER, OrbitCameraState, RenderSettings, SECONDS_PER_DAY,
    SIDE_PANEL_WIDTH_PX, SimulationState, TextureStatus, au_to_scene_units_for_preset,
};
use super::util::format_simulation_speed;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_side_panel(
    mut contexts: EguiContexts,
    time: Res<Time>,
    app_status: Res<AppStatus>,
    mut horizons_sync: ResMut<HorizonsSyncState>,
    mut simulation_state: ResMut<SimulationState>,
    mut render_settings: ResMut<RenderSettings>,
    orbit_camera: Res<OrbitCameraState>,
    texture_status: Res<TextureStatus>,
) -> Result {
    let mode_text = if app_status.spice_enabled {
        "SPICE"
    } else {
        "Fallback"
    };

    egui::SidePanel::left("navigator_side_panel")
        .exact_width(SIDE_PANEL_WIDTH_PX)
        .show(contexts.ctx_mut()?, |ui| {
            ui.heading("Solar Navigator");
            ui.label(format!("Mode: {mode_text}"));
            ui.small(&app_status.status_line);
            ui.small(&horizons_sync.status_line);
            let retry_in_progress = horizons_sync.task.is_some();
            if ui
                .add_enabled(!retry_in_progress, egui::Button::new("Retry Horizons Sync"))
                .clicked()
            {
                horizons_sync.retry_requested = true;
                horizons_sync.retry_attempt = 0;
                horizons_sync.next_retry_deadline_seconds = None;
                horizons_sync.status_line = "Horizons sync retry requested".to_string();
            }
            if retry_in_progress {
                ui.small("Horizons sync request in progress...");
            } else if let Some(deadline) = horizons_sync.next_retry_deadline_seconds {
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

            ui.separator();
            ui.label("Search target:");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut simulation_state.target_filter);
                if ui.button("Clear").clicked() {
                    simulation_state.target_filter.clear();
                }
            });

            let paused_text = if simulation_state.paused {
                "paused"
            } else {
                "running"
            };
            ui.label(format!(
                "Sim: {paused_text} | Speed: {}",
                format_simulation_speed(simulation_state.simulation_rate)
            ));
            ui.small(format!(
                "Days/s equivalent: {:.7}",
                simulation_state.simulation_rate / SECONDS_PER_DAY
            ));
            ui.label(format!("Camera distance: {:.2}", orbit_camera.distance));
            ui.add(
                egui::Slider::new(
                    &mut simulation_state.simulation_rate,
                    MIN_SIMULATION_RATE_MULTIPLIER..=MAX_SIMULATION_RATE_MULTIPLIER,
                )
                .logarithmic(true)
                .text("x realtime"),
            );

            ui.separator();
            ui.label("Lighting preset:");
            egui::ComboBox::from_id_salt("lighting_preset")
                .selected_text(render_settings.preset.label())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut render_settings.preset,
                        super::types::LightingPreset::Navigation,
                        super::types::LightingPreset::Navigation.label(),
                    );
                    ui.selectable_value(
                        &mut render_settings.preset,
                        super::types::LightingPreset::Realistic,
                        super::types::LightingPreset::Realistic.label(),
                    );
                    ui.selectable_value(
                        &mut render_settings.preset,
                        super::types::LightingPreset::Cinematic,
                        super::types::LightingPreset::Cinematic.label(),
                    );
                });
            ui.small(format!(
                "Distance scale: 1 AU = {:.1} units",
                au_to_scene_units_for_preset(render_settings.preset)
            ));

            ui.checkbox(&mut render_settings.stars_enabled, "Starfield backdrop");
            ui.checkbox(&mut render_settings.atmosphere_enabled, "Atmosphere halos");

            ui.separator();
            ui.label("Controls:");
            ui.label("- Left or right drag: orbit");
            ui.label("- Shift + left drag: pan");
            ui.label("- Mouse wheel / trackpad scroll: zoom");
            ui.label("- Space: pause/unpause");
            ui.label("- Up/Down: simulation speed");
            ui.label("- Backspace: reset time/view");

            ui.separator();
            let filter_lc = simulation_state.target_filter.trim().to_ascii_lowercase();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (index, body) in BODIES.iter().enumerate() {
                    let label = body.display_name;
                    if !filter_lc.is_empty() && !label.to_ascii_lowercase().contains(&filter_lc) {
                        continue;
                    }

                    if ui
                        .selectable_label(
                            simulation_state.selected_body_index == Some(index),
                            label,
                        )
                        .clicked()
                    {
                        simulation_state.jump_request = Some(index);
                    }
                }
            });
        });

    Ok(())
}
