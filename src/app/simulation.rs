use super::types::{
    AtmosphereLayer, AtmosphereOf, BODIES, BodyEntity, BodyRuntime, EphemerisResource,
    HorizonsSyncState, MAX_SIMULATION_RATE_MULTIPLIER, MIN_SIMULATION_RATE_MULTIPLIER,
    OrbitCameraState, RenderSettings, SECONDS_PER_DAY, SimulationState,
    au_to_scene_units_for_preset,
};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;

pub(super) fn keyboard_controls(
    key_input: Res<ButtonInput<KeyCode>>,
    egui_input: Res<EguiWantsInput>,
    mut simulation_state: ResMut<SimulationState>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    if egui_input.wants_any_keyboard_input() {
        return;
    }

    if key_input.just_pressed(KeyCode::Space) {
        simulation_state.paused = !simulation_state.paused;
    }
    if key_input.just_pressed(KeyCode::ArrowUp) {
        simulation_state.simulation_rate =
            (simulation_state.simulation_rate * 2.0).min(MAX_SIMULATION_RATE_MULTIPLIER);
    }
    if key_input.just_pressed(KeyCode::ArrowDown) {
        simulation_state.simulation_rate =
            (simulation_state.simulation_rate / 2.0).max(MIN_SIMULATION_RATE_MULTIPLIER);
    }
    if key_input.just_pressed(KeyCode::Backspace) {
        simulation_state.elapsed_simulation_days = 0.0;
        simulation_state.selected_body_index = None;
        simulation_state.jump_request = None;
        orbit_camera.flight = None;
        orbit_camera.target = Vec3::ZERO;
        orbit_camera.distance = 188.3;
    }
}

pub(super) fn advance_simulation_time(
    time: Res<Time>,
    mut simulation_state: ResMut<SimulationState>,
) {
    if !simulation_state.paused {
        simulation_state.elapsed_simulation_days +=
            time.delta_secs_f64() * simulation_state.simulation_rate / SECONDS_PER_DAY;
    }
}

pub(super) fn update_body_positions(
    time: Res<Time>,
    simulation_state: Res<SimulationState>,
    render_settings: Res<RenderSettings>,
    ephemeris: NonSend<EphemerisResource>,
    horizons_sync: Res<HorizonsSyncState>,
    mut body_runtime: ResMut<BodyRuntime>,
    mut body_query: Query<(&BodyEntity, &mut Transform)>,
) {
    let au_to_scene_units = au_to_scene_units_for_preset(render_settings.preset);
    let frame_simulation_seconds = if simulation_state.paused {
        0.0
    } else {
        time.delta_secs() * simulation_state.simulation_rate as f32
    };

    for (body, mut transform) in &mut body_query {
        let spec = BODIES[body.index];
        let position_au = ephemeris
            .ephemeris
            .position_au(spec.spice_target, simulation_state.elapsed_simulation_days);

        let mut scene_position = DVec3::new(
            position_au[0] * au_to_scene_units,
            position_au[2] * au_to_scene_units,
            // Preserve right-handed axes while remapping SPICE Z -> scene Y.
            -position_au[1] * au_to_scene_units,
        );

        if horizons_sync.enabled
            && let Some(offset_au) = horizons_sync.per_body_au_offset.get(body.index)
        {
            scene_position += *offset_au * au_to_scene_units;
        }

        transform.translation = scene_position.as_vec3();
        let spin_step = spin_step_radians(spec.spin_radians_per_second, frame_simulation_seconds);
        if spin_step != 0.0 {
            // After the mesh pre-rotation in setup, local +Z is the visual spin axis.
            // Negating here aligns prograde texture motion with expected planet rotation.
            transform.rotate_local_z(spin_step);
        }

        if let Some(slot) = body_runtime.positions.get_mut(body.index) {
            *slot = scene_position;
        }
    }
}

fn spin_step_radians(spin_radians_per_second: f32, frame_seconds: f32) -> f32 {
    -spin_radians_per_second * frame_seconds
}

pub(super) fn sync_atmosphere_positions(
    body_runtime: Res<BodyRuntime>,
    mut atmosphere_query: Query<(&AtmosphereOf, &mut Transform), With<AtmosphereLayer>>,
) {
    for (atmosphere, mut transform) in &mut atmosphere_query {
        if let Some(position) = body_runtime.positions.get(atmosphere.index) {
            transform.translation = position.as_vec3();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::spin_step_radians;

    #[test]
    fn spin_step_radians_inverts_prograde_sign() {
        assert_eq!(spin_step_radians(0.5, 2.0), -1.0);
    }

    #[test]
    fn spin_step_radians_preserves_retrograde_behavior() {
        assert_eq!(spin_step_radians(-0.5, 2.0), 1.0);
    }

    #[test]
    fn spin_step_radians_zero_rate_is_zero() {
        assert_eq!(spin_step_radians(0.0, 2.0), 0.0);
    }
}
