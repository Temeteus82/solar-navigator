use super::types::{
    BODIES, BodyRuntime, CameraFlight, MainCamera, OrbitCameraState, SimulationState,
};
use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use std::f32::consts::FRAC_PI_2;

const MIN_PITCH: f32 = -FRAC_PI_2 + 0.02;
const MAX_PITCH: f32 = FRAC_PI_2 - 0.02;

pub(super) fn handle_jump_requests(
    mut simulation_state: ResMut<SimulationState>,
    body_runtime: Res<BodyRuntime>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    let Some(target_index) = simulation_state.jump_request.take() else {
        return;
    };

    simulation_state.selected_body_index = Some(target_index);
    let target_distance = compute_target_distance_for_body(BODIES[target_index].visual_radius);

    if let Some(target_position) = body_runtime.positions.get(target_index) {
        orbit_camera.target = target_position.as_vec3();
        orient_camera_toward_sunward(&mut orbit_camera, target_position.as_vec3());
    }
    orbit_camera.distance = target_distance;

    orbit_camera.flight = Some(CameraFlight {
        target_index,
        target_distance,
    });
}

pub(super) fn orbit_camera_input(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut mouse_wheel_events: MessageReader<bevy::input::mouse::MouseWheel>,
    egui_input: Res<EguiWantsInput>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    if egui_input.wants_any_pointer_input() {
        return;
    }

    let mut user_override = false;
    let delta = mouse_motion.delta;
    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    let orbit_drag = mouse_buttons.pressed(MouseButton::Right)
        || (mouse_buttons.pressed(MouseButton::Left) && !shift_held);

    if orbit_drag && delta.length_squared() > 0.01 {
        orbit_camera.yaw -= delta.x * 0.004;
        orbit_camera.pitch -= delta.y * 0.003;
        orbit_camera.pitch = orbit_camera.pitch.clamp(MIN_PITCH, MAX_PITCH);
        user_override = true;
    }

    if shift_held && mouse_buttons.pressed(MouseButton::Left) && delta.length_squared() > 0.01 {
        let forward = Vec3::new(
            orbit_camera.pitch.cos() * orbit_camera.yaw.sin(),
            orbit_camera.pitch.sin(),
            orbit_camera.pitch.cos() * orbit_camera.yaw.cos(),
        )
        .normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = Vec3::Y;
        let pan_scale = (orbit_camera.distance * 0.0024).max(0.0005);
        orbit_camera.target += (-right * delta.x + up * delta.y) * pan_scale;
        user_override = true;
    }

    let mut zoom_steps = 0.0_f32;
    for event in mouse_wheel_events.read() {
        zoom_steps += match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y * 0.004,
        };
    }

    if zoom_steps.abs() > 0.12 {
        let zoom_factor = (1.0 - zoom_steps * 0.12).clamp(0.2, 5.0);
        orbit_camera.distance = (orbit_camera.distance * zoom_factor)
            .clamp(orbit_camera.min_distance, orbit_camera.max_distance);
        user_override = true;
    }

    if user_override {
        orbit_camera.flight = None;
    }
}

pub(super) fn track_selected_body(
    time: Res<Time>,
    simulation_state: Res<SimulationState>,
    body_runtime: Res<BodyRuntime>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    // Keep the selected body centered even while simulation time advances.
    if orbit_camera.flight.is_some() {
        return;
    }

    let Some(selected_index) = simulation_state.selected_body_index else {
        return;
    };

    let Some(target_position) = body_runtime.positions.get(selected_index).copied() else {
        return;
    };

    orbit_camera.target = tracked_target_after_step(
        orbit_camera.target,
        target_position.as_vec3(),
        time.delta_secs(),
    );
}

pub(super) fn apply_camera_flight(
    time: Res<Time>,
    body_runtime: Res<BodyRuntime>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    let Some(flight) = orbit_camera.flight else {
        return;
    };

    let Some(target_at) = body_runtime
        .positions
        .get(flight.target_index)
        .map(|value| value.as_vec3())
    else {
        orbit_camera.flight = None;
        return;
    };

    let dt = time.delta_secs();
    let at_lerp = (dt * 4.0).clamp(0.0, 1.0);
    let dist_lerp = (dt * 3.0).clamp(0.0, 1.0);

    orbit_camera.target = orbit_camera.target.lerp(target_at, at_lerp);
    orbit_camera.distance += (flight.target_distance - orbit_camera.distance) * dist_lerp;

    if orbit_camera.target.distance(target_at) < 0.05
        && (orbit_camera.distance - flight.target_distance).abs() < 0.05
    {
        orbit_camera.flight = None;
    }
}

pub(super) fn update_camera_transform(
    mut camera_query: Query<&mut Transform, (With<MainCamera>, With<Camera3d>)>,
    orbit_camera: Res<OrbitCameraState>,
) {
    let Ok(mut transform) = camera_query.single_mut() else {
        return;
    };

    let x = orbit_camera.distance * orbit_camera.pitch.cos() * orbit_camera.yaw.sin();
    let y = orbit_camera.distance * orbit_camera.pitch.sin();
    let z = orbit_camera.distance * orbit_camera.pitch.cos() * orbit_camera.yaw.cos();

    let translation = orbit_camera.target + Vec3::new(x, y, z);
    *transform = Transform::from_translation(translation).looking_at(orbit_camera.target, Vec3::Y);
}

fn compute_target_distance_for_body(visual_radius: f32) -> f32 {
    (visual_radius * 12.0).clamp(4.0, 120.0)
}

fn orient_camera_toward_sunward(orbit_camera: &mut OrbitCameraState, target_position: Vec3) {
    // When jumping to a planet, place the camera toward the Sun side so
    // the textured day hemisphere is visible instead of a black night side.
    let sunward = (-target_position + Vec3::Y * 0.18).normalize_or_zero();
    if sunward.length_squared() > 0.0 {
        orbit_camera.yaw = sunward.x.atan2(sunward.z);
        orbit_camera.pitch = sunward
            .y
            .clamp(-1.0, 1.0)
            .asin()
            .clamp(MIN_PITCH, MAX_PITCH);
    }
}

fn tracked_target_after_step(current: Vec3, desired: Vec3, delta_seconds: f32) -> Vec3 {
    let lerp_factor = (delta_seconds * 8.0).clamp(0.0, 1.0);
    current.lerp(desired, lerp_factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_target_distance_for_body_clamps_to_expected_bounds() {
        assert_eq!(compute_target_distance_for_body(0.01), 4.0);
        assert_eq!(compute_target_distance_for_body(2.0), 24.0);
        assert_eq!(compute_target_distance_for_body(20.0), 120.0);
    }

    #[test]
    fn orient_camera_toward_sunward_keeps_pitch_in_valid_range() {
        let mut camera = OrbitCameraState {
            yaw: 0.0,
            pitch: 0.0,
            distance: 10.0,
            min_distance: 1.0,
            max_distance: 100.0,
            target: Vec3::ZERO,
            flight: None,
        };

        orient_camera_toward_sunward(&mut camera, Vec3::new(12.0, 3.0, -4.0));

        assert!(camera.yaw.is_finite());
        assert!(camera.pitch.is_finite());
        assert!((MIN_PITCH..=MAX_PITCH).contains(&camera.pitch));
    }

    #[test]
    fn tracked_target_after_step_moves_toward_desired_target() {
        let current = Vec3::new(0.0, 0.0, 0.0);
        let desired = Vec3::new(10.0, -2.0, 4.0);
        let next = tracked_target_after_step(current, desired, 0.1);

        assert!(next.distance(current) > 0.0);
        assert!(next.distance(desired) < current.distance(desired));
    }

    #[test]
    fn tracked_target_after_step_reaches_desired_on_large_delta() {
        let desired = Vec3::new(10.0, -2.0, 4.0);
        assert_eq!(
            tracked_target_after_step(Vec3::ZERO, desired, 100.0),
            desired
        );
    }
}
