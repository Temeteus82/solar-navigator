use super::types::{
    AppStatus, AtmosphereLayer, BODIES, LightingRig, RenderSettings, SimulationState, StarsBackdrop,
};
use super::util::format_simulation_speed;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

pub(super) fn apply_lighting_preset(
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

    // Realistic lighting: the Sun is the sole key light at scene origin.
    // Inverse-square falloff produces a natural brightness gradient — Mercury
    // and Venus are bright, outer planets are dim. Ambient and fill are kept
    // low so the gradient is not washed out.
    ambient.brightness = 0.3;

    solar_key.intensity = 1_600_000_000.0;
    solar_key.color = Color::srgb(1.0, 0.97, 0.9);
    solar_key.shadows_enabled = false;

    sky_fill.illuminance = 5.0;
    sky_fill.color = Color::srgb(0.3, 0.35, 0.45);

    rim_fill.intensity = 0.0;
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
        "Solar Navigator [{mode}] | Speed: {}{selection_label}",
        format_simulation_speed(simulation_state.simulation_rate),
    );
}
