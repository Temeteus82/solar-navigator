use super::types::{
    AU_TO_SCENE_UNITS, AppStatus, AtmosphereLayer, BODIES, BodyRuntime, BodyTrails, CameraMode,
    LightingRig, OrbitCameraState, PlanetRing, RenderSettings, SimulationState, StarsBackdrop,
    TRAIL_MAX_POINTS,
};
use super::util::format_simulation_speed;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::f32::consts::TAU;

pub(super) fn apply_lighting_preset(
    lighting_rig: Res<LightingRig>,
    orbit_camera: Res<OrbitCameraState>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut point_lights: Query<&mut PointLight>,
    mut directional_lights: Query<&mut DirectionalLight>,
    mut transforms: Query<&mut Transform>,
) {
    let Ok([mut solar_key, mut rim_fill]) =
        point_lights.get_many_mut([lighting_rig.solar_key, lighting_rig.rim_fill])
    else {
        return;
    };
    let Ok(mut sky_fill) = directional_lights.get_mut(lighting_rig.sky_fill) else {
        return;
    };

    // The Sun's primary illumination is delivered by `sky_fill` as a
    // DirectionalLight (no inverse-square falloff), so every planet
    // receives equal sunlight regardless of its distance from origin.
    // Each frame the light direction is updated to point from the Sun
    // toward whatever the camera is focused on, keeping the lit
    // hemisphere of the inspected body correctly oriented.
    // Aim the sun toward whatever the viewer is focused on: the orbit target,
    // or the free camera's own position (so the camera-facing hemisphere of
    // nearby bodies stays lit while flying around).
    let focus = match orbit_camera.mode {
        CameraMode::Orbit => orbit_camera.target,
        CameraMode::Free => orbit_camera.free_position,
    };
    let sun_dir = if focus.length_squared() > 1e-4 {
        focus.normalize()
    } else {
        Vec3::new(1.0, -0.05, 0.0).normalize()
    };
    if let Ok(mut transform) = transforms.get_mut(lighting_rig.sky_fill) {
        *transform = Transform::IDENTITY.looking_to(sun_dir, Vec3::Y);
    }

    sky_fill.illuminance = 1_800.0;
    sky_fill.color = Color::srgb(1.0, 0.97, 0.9);
    sky_fill.shadow_maps_enabled = true;

    // Point light at the Sun is kept dim — its only role is to add a
    // small amount of inner-system specular detail and bloom near the
    // Sun itself. The directional light handles the actual planet shading.
    solar_key.intensity = 80_000_000.0;
    solar_key.color = Color::srgb(1.0, 0.97, 0.9);
    solar_key.shadow_maps_enabled = false;

    ambient.brightness = 0.25;

    rim_fill.intensity = 0.0;
}

#[allow(clippy::type_complexity)]
pub(super) fn sync_visibility_toggles(
    render_settings: Res<RenderSettings>,
    mut atmosphere_query: Query<
        &mut Visibility,
        (
            With<AtmosphereLayer>,
            Without<StarsBackdrop>,
            Without<PlanetRing>,
        ),
    >,
    mut stars_query: Query<
        &mut Visibility,
        (
            With<StarsBackdrop>,
            Without<AtmosphereLayer>,
            Without<PlanetRing>,
        ),
    >,
    mut ring_query: Query<
        &mut Visibility,
        (
            With<PlanetRing>,
            Without<AtmosphereLayer>,
            Without<StarsBackdrop>,
        ),
    >,
) {
    let atmosphere_visibility = visibility_for(render_settings.atmosphere_enabled);
    let stars_visibility = visibility_for(render_settings.stars_enabled);
    let ring_visibility = visibility_for(render_settings.rings_enabled);

    for mut visibility in &mut atmosphere_query {
        *visibility = atmosphere_visibility;
    }
    for mut visibility in &mut stars_query {
        *visibility = stars_visibility;
    }
    for mut visibility in &mut ring_query {
        *visibility = ring_visibility;
    }
}

fn visibility_for(enabled: bool) -> Visibility {
    if enabled {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

pub(super) fn record_body_trails(
    simulation_state: Res<SimulationState>,
    render_settings: Res<RenderSettings>,
    body_runtime: Res<BodyRuntime>,
    mut trails: ResMut<BodyTrails>,
) {
    if !render_settings.trails_enabled || simulation_state.paused {
        return;
    }

    for (index, position) in body_runtime.positions.iter().enumerate() {
        let Some(trail) = trails.points.get_mut(index) else {
            continue;
        };
        let sample = position.as_vec3();
        // Skip duplicates (avoids zero-length trail segments when camera is idle).
        if trail
            .back()
            .is_some_and(|last| last.distance_squared(sample) < 1e-6)
        {
            continue;
        }
        trail.push_back(sample);
        while trail.len() > TRAIL_MAX_POINTS {
            trail.pop_front();
        }
    }
}

pub(super) fn draw_body_trails(
    render_settings: Res<RenderSettings>,
    trails: Res<BodyTrails>,
    mut gizmos: Gizmos,
) {
    if !render_settings.trails_enabled {
        return;
    }

    for (index, trail) in trails.points.iter().enumerate() {
        if trail.len() < 2 {
            continue;
        }
        let Some(spec) = BODIES.get(index) else {
            continue;
        };
        let base = Color::srgba(spec.color[0], spec.color[1], spec.color[2], 0.9);
        gizmos.linestrip(trail.iter().copied(), base);
    }
}

pub(super) fn draw_orbit_paths(
    render_settings: Res<RenderSettings>,
    simulation_state: Res<SimulationState>,
    mut gizmos: Gizmos,
) {
    if !render_settings.orbits_enabled {
        return;
    }
    for (index, spec) in BODIES.iter().enumerate() {
        let Some(sma_au) = spec.semi_major_axis_au else {
            continue;
        };
        let radius = sma_au as f32 * AU_TO_SCENE_UNITS as f32;
        let is_selected = simulation_state.selected_body_index == Some(index);
        let alpha = if is_selected { 0.55 } else { 0.12 };
        let color = Color::srgba(spec.color[0], spec.color[1], spec.color[2], alpha);
        // Feed the ring points straight into the gizmo iterator instead of
        // collecting into a per-frame Vec for every body.
        gizmos.linestrip(
            (0..=128).map(|i| {
                let a = i as f32 / 128.0 * TAU;
                Vec3::new(radius * a.cos(), 0.0, radius * a.sin())
            }),
            color,
        );
    }
}

pub(super) fn update_window_title(
    app_status: Res<AppStatus>,
    simulation_state: Res<SimulationState>,
    orbit_camera: Res<OrbitCameraState>,
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

    let camera_label = match orbit_camera.mode {
        CameraMode::Orbit => "",
        CameraMode::Free => " | Free cam",
    };

    let selection_label = simulation_state
        .selected_body_index
        .map(|idx| format!(" | Selected: {}", BODIES[idx].display_name))
        .unwrap_or_default();

    window.title = format!(
        "Solar Navigator [{mode}] | Speed: {}{camera_label}{selection_label}",
        format_simulation_speed(simulation_state.simulation_rate),
    );
}
