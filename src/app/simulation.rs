use super::types::{
    AU_TO_SCENE_UNITS, AtmosphereLayer, AtmosphereOf, BODIES, BodyEntity, BodyRuntime,
    EphemerisResource, HorizonsSyncState, MAX_SIMULATION_RATE_MULTIPLIER,
    MIN_SIMULATION_RATE_MULTIPLIER, OrbitCameraState, SECONDS_PER_DAY, SimulationState,
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
    ephemeris: NonSend<EphemerisResource>,
    horizons_sync: Res<HorizonsSyncState>,
    mut body_runtime: ResMut<BodyRuntime>,
    mut body_query: Query<(&BodyEntity, &mut Transform)>,
) {
    let frame_seconds = time.delta_secs();

    for (body, mut transform) in &mut body_query {
        let spec = BODIES[body.index];
        let position_au = ephemeris
            .ephemeris
            .position_au(spec.spice_target, simulation_state.elapsed_simulation_days);

        let mut scene_position = DVec3::new(
            position_au[0] * AU_TO_SCENE_UNITS,
            position_au[2] * AU_TO_SCENE_UNITS,
            // Preserve right-handed axes while remapping SPICE Z -> scene Y.
            -position_au[1] * AU_TO_SCENE_UNITS,
        );

        if horizons_sync.enabled
            && let Some(offset) = horizons_sync.per_body_scene_offset.get(body.index)
        {
            scene_position += *offset;
        }

        transform.translation = scene_position.as_vec3();
        if spec.spin_radians_per_second > 0.0 {
            transform.rotate_local_z(spec.spin_radians_per_second * frame_seconds);
        }

        if let Some(slot) = body_runtime.positions.get_mut(body.index) {
            *slot = scene_position;
        }
    }
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
