use super::types::{
    AppStatus, AtmosphereLayer, BODIES, LightingPreset, LightingRig, RenderSettings,
    SimulationState, StarsBackdrop,
};
use super::util::format_simulation_speed;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub(super) fn apply_lighting_preset(
    render_settings: Res<RenderSettings>,
    lighting_rig: Res<LightingRig>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut point_lights: Query<&mut PointLight>,
    mut directional_lights: Query<&mut DirectionalLight>,
) {
    let Ok([mut solar_key, mut rim_fill]) =
        point_lights.get_many_mut([lighting_rig.solar_key, lighting_rig.rim_fill])
    else {
        return;
    };
    let Ok(mut sky_fill) = directional_lights.get_mut(lighting_rig.sky_fill) else {
        return;
    };

    match render_settings.preset {
        LightingPreset::Navigation => {
            ambient.brightness = 9.0;

            solar_key.intensity = 880_000_000.0;
            solar_key.color = Color::srgb(1.0, 0.97, 0.9);
            solar_key.shadows_enabled = false;

            sky_fill.illuminance = 95.0;
            sky_fill.color = Color::srgb(0.72, 0.8, 0.9);

            rim_fill.intensity = 0.0;
        }
        LightingPreset::Realistic => {
            // Keep space mostly dark and let the Sun dominate. Lower non-solar fill
            // preserves texture contrast so planets don't look washed out.
            ambient.brightness = 1.2;

            solar_key.intensity = 820_000_000.0;
            solar_key.color = Color::srgb(1.0, 0.95, 0.86);
            solar_key.shadows_enabled = false;

            sky_fill.illuminance = 18.0;
            sky_fill.color = Color::srgb(0.38, 0.44, 0.54);

            rim_fill.intensity = 0.0;
        }
        LightingPreset::Cinematic => {
            ambient.brightness = 5.0;

            solar_key.intensity = 1_150_000_000.0;
            solar_key.color = Color::srgb(1.0, 0.9, 0.78);
            solar_key.shadows_enabled = false;

            sky_fill.illuminance = 90.0;
            sky_fill.color = Color::srgb(0.46, 0.6, 0.82);

            rim_fill.intensity = 12_000.0;
            rim_fill.color = Color::srgb(0.47, 0.55, 0.78);
        }
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn sync_visibility_toggles(
    render_settings: Res<RenderSettings>,
    mut visibility_query: Query<
        (
            &mut Visibility,
            Option<&AtmosphereLayer>,
            Option<&StarsBackdrop>,
        ),
        Or<(With<AtmosphereLayer>, With<StarsBackdrop>)>,
    >,
) {
    let atmosphere_visibility = if render_settings.atmosphere_enabled {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    let stars_visibility = if render_settings.stars_enabled {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for (mut visibility, atmosphere, stars) in &mut visibility_query {
        if atmosphere.is_some() {
            *visibility = atmosphere_visibility;
        }
        if stars.is_some() {
            *visibility = stars_visibility;
        }
    }
}

pub(super) fn update_window_title(
    app_status: Res<AppStatus>,
    simulation_state: Res<SimulationState>,
    render_settings: Res<RenderSettings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };

    let mode = if app_status.spice_enabled {
        "SPICE"
    } else {
        "Fallback"
    };

    let selection_label = simulation_state
        .selected_body_index
        .map(|idx| format!(" | Selected: {}", BODIES[idx].display_name))
        .unwrap_or_default();

    window.title = format!(
        "Solar Navigator [{mode}] | {} lighting | Speed: {}{selection_label}",
        render_settings.preset.label(),
        format_simulation_speed(simulation_state.simulation_rate),
    );
}
