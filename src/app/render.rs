use super::camera::view_focus_distance;
use super::materials::{PlanetAtmosphereMaterial, PlanetRingMaterial};
use super::types::{
    AU_TO_SCENE_UNITS, AppStatus, AtmosphereLayer, BODIES, BodyRuntime, BodyTrails, CameraMode,
    LightingRig, MAX_FRAME_RATE, MainCamera, OrbitCameraState, PlanetRing, RenderOrigin,
    RenderSettings, SimulationState, StarsBackdrop, TRAIL_MAX_POINTS, WorldPosition,
};
use super::util::format_simulation_speed;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::math::DVec3;
use bevy::pbr::{ContactShadows, ScreenSpaceAmbientOcclusion};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::f64::consts::TAU;
use std::time::{Duration, Instant};

pub(super) fn apply_lighting_preset(
    lighting_rig: Res<LightingRig>,
    orbit_camera: Res<OrbitCameraState>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut point_lights: Query<&mut PointLight>,
    mut directional_lights: Query<&mut DirectionalLight>,
    mut transforms: Query<&mut Transform>,
) {
    let Ok(mut solar_key) = point_lights.get_mut(lighting_rig.solar_key) else {
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
    let focus = focus.as_vec3();
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
}

/// Bevy's default first shadow cascade ends this far from the camera. Its
/// world-unit defaults for shadow cascades, depth bias, SSAO and contact
/// shadows are all tuned for a view at roughly this scale.
const DEFAULT_FIRST_CASCADE_FAR_BOUND: f32 = 10.0;

/// The first (sharpest) shadow cascade reaches this multiple of the focus
/// distance, so it covers the inspected body and whatever casts onto it.
const FIRST_CASCADE_FOCUS_MULTIPLE: f32 = 1.5;

/// Factor by which Bevy's world-unit defaults are scaled for a viewer
/// `focus_distance` from their subject.
fn view_effect_scale(focus_distance: f32) -> f32 {
    focus_distance * FIRST_CASCADE_FOCUS_MULTIPLE / DEFAULT_FIRST_CASCADE_FAR_BOUND
}

/// Scales the sun's shadow cascades and depth bias, SSAO thickness and
/// contact-shadow ray length with the camera's focus distance.
///
/// Bevy's defaults are fixed world-unit values tuned for a ~10-unit view, but
/// here the view spans from Voyager close-ups (~0.036 units per metre, so the
/// default 0.02 depth bias alone is ~56 cm on the model) to the whole solar
/// system. Scaling the entire default set uniformly keeps its tuned ratios —
/// e.g. depth bias ≈ 3 shadow-map texels — at every zoom level, and puts the
/// sharpest cascade's texels where the viewer is actually looking.
pub(super) fn scale_view_dependent_effects(
    lighting_rig: Res<LightingRig>,
    orbit_camera: Res<OrbitCameraState>,
    body_runtime: Res<BodyRuntime>,
    mut lights: Query<(&mut DirectionalLight, &mut CascadeShadowConfig)>,
    mut cameras: Query<(&mut ScreenSpaceAmbientOcclusion, &mut ContactShadows), With<MainCamera>>,
) {
    let scale = view_effect_scale(view_focus_distance(&orbit_camera, &body_runtime));
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }

    // Compare before writing: a write marks the component changed, and the
    // focus distance is often static between frames.
    if let Ok((mut light, mut cascades)) = lights.get_mut(lighting_rig.sky_fill) {
        let depth_bias = DirectionalLight::DEFAULT_SHADOW_DEPTH_BIAS * scale;
        if light.shadow_depth_bias != depth_bias {
            light.shadow_depth_bias = depth_bias;
        }
        let defaults = CascadeShadowConfigBuilder::default();
        let scaled: CascadeShadowConfig = CascadeShadowConfigBuilder {
            minimum_distance: defaults.minimum_distance * scale,
            maximum_distance: defaults.maximum_distance * scale,
            first_cascade_far_bound: defaults.first_cascade_far_bound * scale,
            ..defaults
        }
        .into();
        if cascades.bounds != scaled.bounds || cascades.minimum_distance != scaled.minimum_distance
        {
            *cascades = scaled;
        }
    }

    for (mut ssao, mut contact_shadows) in &mut cameras {
        let thickness = ScreenSpaceAmbientOcclusion::default().constant_object_thickness * scale;
        if ssao.constant_object_thickness != thickness {
            ssao.constant_object_thickness = thickness;
        }
        let defaults = ContactShadows::default();
        let (contact_thickness, contact_length) =
            (defaults.thickness * scale, defaults.length * scale);
        if contact_shadows.thickness != contact_thickness
            || contact_shadows.length != contact_length
        {
            contact_shadows.thickness = contact_thickness;
            contact_shadows.length = contact_length;
        }
    }
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

/// Floating origin, final step: turns every `WorldPosition` into a
/// render-space translation relative to `RenderOrigin` (the camera). The
/// subtraction happens in f64, so only the small camera-relative offset is
/// ever rounded to f32 — Voyager's model keeps sub-millimetre vertex
/// precision ~170 AU out instead of snapping to an ~11 cm grid.
///
/// Runs in `PostUpdate` ahead of transform propagation, so it sees the final
/// world positions and origin of the frame whatever order `Update` ran in.
/// The sky sphere carries no `WorldPosition`: left at the render origin it
/// stays centred on the camera like a skybox, which also keeps it around
/// Voyager, far beyond `STARFIELD_RADIUS` from the Sun.
pub(super) fn apply_render_origin(
    render_origin: Res<RenderOrigin>,
    mut query: Query<(&WorldPosition, &mut Transform)>,
) {
    for (world_position, mut transform) in &mut query {
        transform.translation = render_space(world_position.0, render_origin.0);
    }
}

/// `world` relative to the floating origin, rounded to f32 only after the
/// subtraction.
fn render_space(world: DVec3, origin: DVec3) -> Vec3 {
    (world - origin).as_vec3()
}

/// The custom atmosphere and ring shaders light from the Sun, which sits at
/// the world origin — at `-RenderOrigin` in render space. Push that position
/// into their `sun_position` uniforms whenever the origin moves.
pub(super) fn sync_shader_sun_positions(
    render_origin: Res<RenderOrigin>,
    mut atmosphere_materials: ResMut<Assets<PlanetAtmosphereMaterial>>,
    mut ring_materials: ResMut<Assets<PlanetRingMaterial>>,
) {
    let sun = render_space(DVec3::ZERO, render_origin.0).extend(0.0);
    // Check before taking `&mut`: a mutable access re-uploads the material.
    let stale_atmospheres: Vec<_> = atmosphere_materials
        .iter()
        .filter(|(_, material)| material.sun_position != sun)
        .map(|(id, _)| id)
        .collect();
    for id in stale_atmospheres {
        if let Some(mut material) = atmosphere_materials.get_mut(id) {
            material.sun_position = sun;
        }
    }
    let stale_rings: Vec<_> = ring_materials
        .iter()
        .filter(|(_, material)| material.sun_position != sun)
        .map(|(id, _)| id)
        .collect();
    for id in stale_rings {
        if let Some(mut material) = ring_materials.get_mut(id) {
            material.sun_position = sun;
        }
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
        let sample = *position;
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
    render_origin: Res<RenderOrigin>,
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
        gizmos.linestrip(
            trail
                .iter()
                .map(|&point| render_space(point, render_origin.0)),
            base,
        );
    }
}

pub(super) fn draw_orbit_paths(
    render_settings: Res<RenderSettings>,
    simulation_state: Res<SimulationState>,
    render_origin: Res<RenderOrigin>,
    mut gizmos: Gizmos,
) {
    if !render_settings.orbits_enabled {
        return;
    }
    for (index, spec) in BODIES.iter().enumerate() {
        let Some(sma_au) = spec.semi_major_axis_au else {
            continue;
        };
        let radius = sma_au * AU_TO_SCENE_UNITS;
        let is_selected = simulation_state.selected_body_index == Some(index);
        let alpha = if is_selected { 0.55 } else { 0.12 };
        let color = Color::srgba(spec.color[0], spec.color[1], spec.color[2], alpha);
        // Feed the ring points straight into the gizmo iterator instead of
        // collecting into a per-frame Vec for every body.
        gizmos.linestrip(
            (0..=128).map(|i| {
                let a = f64::from(i) / 128.0 * TAU;
                render_space(
                    DVec3::new(radius * a.cos(), 0.0, radius * a.sin()),
                    render_origin.0,
                )
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

/// Sleeps out the rest of the frame so the app never renders faster than
/// `MAX_FRAME_RATE`. Runs last in the main schedule; with pipelined rendering
/// the render world can't get ahead of the main world, so this paces the GPU too.
pub(super) fn cap_frame_rate(mut next_frame: Local<Option<Instant>>) {
    let now = Instant::now();
    let (sleep, next) = frame_pacing(
        now,
        *next_frame,
        Duration::from_secs_f64(1.0 / MAX_FRAME_RATE),
    );
    if let Some(sleep) = sleep {
        std::thread::sleep(sleep);
    }
    *next_frame = Some(next);
}

/// How long to sleep now, and when the following frame is due. Deadlines
/// advance by whole frame periods so pacing doesn't drift; a frame that
/// overran its deadline resets the schedule instead of trying to catch up.
fn frame_pacing(
    now: Instant,
    deadline: Option<Instant>,
    period: Duration,
) -> (Option<Duration>, Instant) {
    match deadline {
        Some(deadline) if deadline > now => (Some(deadline - now), deadline + period),
        _ => (None, now + period),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_pacing_sleeps_until_deadline_and_advances_by_one_period() {
        let now = Instant::now();
        let period = Duration::from_millis(16);
        let deadline = now + Duration::from_millis(5);
        let (sleep, next) = frame_pacing(now, Some(deadline), period);
        assert_eq!(sleep, Some(Duration::from_millis(5)));
        assert_eq!(next, deadline + period);
    }

    #[test]
    fn frame_pacing_resets_after_an_overrun_or_on_the_first_frame() {
        let now = Instant::now();
        let period = Duration::from_millis(16);
        for deadline in [None, Some(now), Some(now - Duration::from_millis(40))] {
            let (sleep, next) = frame_pacing(now, deadline, period);
            assert_eq!(sleep, None);
            assert_eq!(next, now + period);
        }
    }

    #[test]
    fn render_space_keeps_precision_far_from_the_sun() {
        // A vertex 1 mm (in model metres, at Voyager's ~0.036 units/m scale)
        // from a camera ~170 AU out. In plain f32 world space both would snap
        // to the same 0.004-unit grid point; relative to the origin the offset
        // survives intact.
        let origin = DVec3::new(42_500.0, -1_234.5, 7_777.25);
        let offset = DVec3::new(0.000_036, 0.0, -0.000_036);

        let rendered = render_space(origin + offset, origin);

        assert!((rendered.as_dvec3() - offset).length() < 1e-9);
        assert_eq!(
            (origin + offset).as_vec3(),
            origin.as_vec3(),
            "test premise: f32 world coordinates cannot resolve the offset"
        );
    }

    #[test]
    fn view_effect_scale_matches_bevy_defaults_at_their_reference_view() {
        // A view whose first cascade lands on Bevy's default bound keeps
        // Bevy's defaults unchanged.
        let reference_focus = DEFAULT_FIRST_CASCADE_FAR_BOUND / FIRST_CASCADE_FOCUS_MULTIPLE;
        assert!((view_effect_scale(reference_focus) - 1.0).abs() < 1e-6);
        assert_eq!(
            CascadeShadowConfigBuilder::default().first_cascade_far_bound,
            DEFAULT_FIRST_CASCADE_FAR_BOUND,
            "Bevy's default first cascade bound moved; retune the reference"
        );
    }

    #[test]
    fn view_effect_scale_is_proportional_to_focus_distance() {
        assert!((view_effect_scale(0.6) / view_effect_scale(6.0) - 0.1).abs() < 1e-6);
    }
}
