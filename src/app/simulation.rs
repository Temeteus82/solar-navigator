use super::types::{
    AU_TO_SCENE_UNITS, AtmosphereLayer, AtmosphereOf, BODIES, BodyEntity, BodyRuntime, BodyTrails,
    EphemerisResource, HorizonsSyncState, KM_PER_AU, MAX_SIMULATION_RATE_MULTIPLIER,
    MIN_SIMULATION_RATE_MULTIPLIER, OrbitCameraState, SECONDS_PER_DAY, SimulationState,
};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use std::f64::consts::TAU;

const CHARON_SEMI_MAJOR_AXIS_KM: f64 = 19_591.0;
const CHARON_ORBIT_PERIOD_DAYS: f64 = 6.38723;
const CHARON_ORBIT_PHASE_RADIANS: f64 = 1.1;
const CHARON_Z_WOBBLE_FACTOR: f64 = 0.03;
const CHARON_Z_WOBBLE_FREQUENCY: f64 = 1.4;
const CHARON_TO_PLUTO_MASS_RATIO: f64 = 0.1218;

pub(super) fn keyboard_controls(
    key_input: Res<ButtonInput<KeyCode>>,
    egui_input: Res<EguiWantsInput>,
    mut simulation_state: ResMut<SimulationState>,
    mut orbit_camera: ResMut<OrbitCameraState>,
    mut trails: ResMut<BodyTrails>,
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
        trails.clear();
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
    let au_to_scene_units = AU_TO_SCENE_UNITS;
    let frame_simulation_seconds = if simulation_state.paused {
        0.0
    } else {
        time.delta_secs() * simulation_state.simulation_rate as f32
    };
    let mut scene_positions = vec![DVec3::ZERO; BODIES.len()];

    for body_index in 0..BODIES.len() {
        let spec = BODIES[body_index];
        let position_au = ephemeris
            .ephemeris
            .position_au(spec.spice_target, simulation_state.elapsed_simulation_days);

        scene_positions[body_index] = DVec3::new(
            position_au[0] * au_to_scene_units,
            position_au[2] * au_to_scene_units,
            // Preserve right-handed axes while remapping SPICE Z -> scene Y.
            -position_au[1] * au_to_scene_units,
        );

        if horizons_sync.enabled
            && let Some(offset_au) = horizons_sync.per_body_au_offset.get(body_index)
        {
            scene_positions[body_index] += *offset_au * au_to_scene_units;
        }
    }

    apply_pluto_charon_center_positions(
        &mut scene_positions,
        simulation_state.elapsed_simulation_days,
        au_to_scene_units,
    );

    for (body, mut transform) in &mut body_query {
        let spec = BODIES[body.index];
        let scene_position = scene_positions[body.index];

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

fn body_index_for_target(target: &str) -> Option<usize> {
    BODIES.iter().position(|spec| spec.spice_target == target)
}

fn charon_relative_scene_offset(elapsed_simulation_days: f64, au_to_scene_units: f64) -> DVec3 {
    let radius_scene_units = (CHARON_SEMI_MAJOR_AXIS_KM / KM_PER_AU) * au_to_scene_units;
    let theta =
        TAU * elapsed_simulation_days / CHARON_ORBIT_PERIOD_DAYS + CHARON_ORBIT_PHASE_RADIANS;

    DVec3::new(
        radius_scene_units * theta.cos(),
        radius_scene_units * CHARON_Z_WOBBLE_FACTOR * (theta * CHARON_Z_WOBBLE_FREQUENCY).sin(),
        -radius_scene_units * theta.sin(),
    )
}

fn apply_pluto_charon_center_positions(
    scene_positions: &mut [DVec3],
    elapsed_simulation_days: f64,
    au_to_scene_units: f64,
) {
    let Some(pluto_barycenter_index) = body_index_for_target("PLUTO BARYCENTER") else {
        return;
    };
    let Some(charon_index) = body_index_for_target("CHARON") else {
        return;
    };

    let pluto_charon_barycenter = scene_positions[pluto_barycenter_index];
    let charon_from_pluto =
        charon_relative_scene_offset(elapsed_simulation_days, au_to_scene_units);
    let charon_mass_fraction = CHARON_TO_PLUTO_MASS_RATIO / (1.0 + CHARON_TO_PLUTO_MASS_RATIO);
    let pluto_mass_fraction = 1.0 - charon_mass_fraction;

    // Reconstruct Pluto/Charon center positions from the Pluto-Charon barycenter so
    // separation remains on the same physical scale as all other AU-derived distances.
    scene_positions[pluto_barycenter_index] =
        pluto_charon_barycenter - charon_from_pluto * charon_mass_fraction;
    scene_positions[charon_index] =
        pluto_charon_barycenter + charon_from_pluto * pluto_mass_fraction;
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
    use super::{
        CHARON_TO_PLUTO_MASS_RATIO, apply_pluto_charon_center_positions, body_index_for_target,
        charon_relative_scene_offset, spin_step_radians,
    };
    use bevy::math::DVec3;

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

    #[test]
    fn apply_pluto_charon_center_positions_preserves_barycenter_and_separation() {
        let mut positions = vec![DVec3::ZERO; super::BODIES.len()];
        let pluto_index =
            body_index_for_target("PLUTO BARYCENTER").expect("pluto index should exist");
        let charon_index = body_index_for_target("CHARON").expect("charon index should exist");
        let original_barycenter = DVec3::new(12.0, -4.0, 8.0);
        positions[pluto_index] = original_barycenter;

        let elapsed_days = 42.0;
        let au_to_scene_units = 250.0;
        apply_pluto_charon_center_positions(&mut positions, elapsed_days, au_to_scene_units);

        let pluto = positions[pluto_index];
        let charon = positions[charon_index];
        let expected_separation =
            charon_relative_scene_offset(elapsed_days, au_to_scene_units).length();
        let actual_separation = (charon - pluto).length();
        let reconstructed_barycenter =
            (pluto + charon * CHARON_TO_PLUTO_MASS_RATIO) / (1.0 + CHARON_TO_PLUTO_MASS_RATIO);

        assert!((actual_separation - expected_separation).abs() < 1e-12);
        assert!((reconstructed_barycenter - original_barycenter).length() < 1e-12);
    }
}
