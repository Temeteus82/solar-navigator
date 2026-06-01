use super::types::{
    BODIES, BodyRuntime, CameraFlight, CameraMode, FREE_CAMERA_BOOST_MULTIPLIER,
    FREE_CAMERA_LOOK_SENSITIVITY, FREE_CAMERA_MAX_SPEED, FREE_CAMERA_MIN_SPEED,
    FREE_CAMERA_SPEED_FACTOR, MainCamera, OrbitCameraState, SimulationState,
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

    // Selecting a body always returns to the orbit camera (e.g. clicking a
    // body in the list while flying around in Free mode).
    orbit_camera.mode = CameraMode::Orbit;
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

/// `F` toggles between the orbit and free-fly cameras. Both directions hand
/// off seamlessly: entering Free seeds the fly-cam from the orbit camera's
/// current pose; returning to Orbit re-tethers to the selected (or nearest)
/// body without snapping the view.
pub(super) fn toggle_camera_mode(
    key_input: Res<ButtonInput<KeyCode>>,
    egui_input: Res<EguiWantsInput>,
    mut simulation_state: ResMut<SimulationState>,
    body_runtime: Res<BodyRuntime>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    if egui_input.wants_any_keyboard_input() {
        return;
    }
    if key_input.just_pressed(KeyCode::KeyF) {
        toggle_camera_mode_impl(&mut orbit_camera, &mut simulation_state, &body_runtime);
    }
}

/// Shared mode-switch logic, called from both the `F` key and the UI button.
pub(super) fn toggle_camera_mode_impl(
    orbit_camera: &mut OrbitCameraState,
    simulation_state: &mut SimulationState,
    body_runtime: &BodyRuntime,
) {
    match orbit_camera.mode {
        CameraMode::Orbit => {
            // Seed the fly-cam from the orbit camera's current world pose so
            // the switch is invisible until the user moves.
            let position = orbit_camera_world_position(orbit_camera);
            let look_dir = (orbit_camera.target - position).normalize_or_zero();
            let (yaw, pitch) = look_angles_from_direction(look_dir);
            orbit_camera.free_position = position;
            orbit_camera.free_yaw = yaw;
            orbit_camera.free_pitch = pitch;
            orbit_camera.flight = None;
            orbit_camera.mode = CameraMode::Free;
        }
        CameraMode::Free => {
            // Re-tether to the selected body, or the nearest one if nothing is
            // selected, preserving the current position/orientation.
            let anchor = simulation_state
                .selected_body_index
                .filter(|&i| body_runtime.positions.get(i).is_some())
                .or_else(|| nearest_body_index(body_runtime, orbit_camera.free_position));

            if let Some(index) = anchor {
                let target = body_runtime.positions[index].as_vec3();
                let offset = orbit_camera.free_position - target;
                let distance = offset
                    .length()
                    .clamp(orbit_camera.min_distance, orbit_camera.max_distance);
                let (yaw, pitch) = look_angles_from_direction(offset.normalize_or_zero());
                orbit_camera.target = target;
                orbit_camera.distance = distance;
                orbit_camera.yaw = yaw;
                orbit_camera.pitch = pitch.clamp(MIN_PITCH, MAX_PITCH);
                simulation_state.selected_body_index = Some(index);
            }
            orbit_camera.flight = None;
            orbit_camera.mode = CameraMode::Orbit;
        }
    }
}

pub(super) fn orbit_camera_input(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    mut mouse_wheel_events: MessageReader<bevy::input::mouse::MouseWheel>,
    egui_input: Res<EguiWantsInput>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    if orbit_camera.mode != CameraMode::Orbit {
        return;
    }
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

/// Free-fly camera: WASD to move in the look plane, Q/E for down/up, drag to
/// look. Speed auto-scales with the distance to the nearest body; hold Shift
/// to boost. Only active in `CameraMode::Free`.
pub(super) fn free_camera_input(
    time: Res<Time>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<bevy::input::mouse::AccumulatedMouseMotion>,
    egui_input: Res<EguiWantsInput>,
    body_runtime: Res<BodyRuntime>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    if orbit_camera.mode != CameraMode::Free {
        return;
    }

    // Mouse-look on any drag — there is no orbit pivot to drag against here.
    if !egui_input.wants_any_pointer_input() {
        let dragging =
            mouse_buttons.pressed(MouseButton::Left) || mouse_buttons.pressed(MouseButton::Right);
        let delta = mouse_motion.delta;
        if dragging && delta.length_squared() > 0.01 {
            orbit_camera.free_yaw -= delta.x * FREE_CAMERA_LOOK_SENSITIVITY;
            orbit_camera.free_pitch = (orbit_camera.free_pitch
                - delta.y * FREE_CAMERA_LOOK_SENSITIVITY)
                .clamp(MIN_PITCH, MAX_PITCH);
        }
    }

    if egui_input.wants_any_keyboard_input() {
        return;
    }

    let forward = direction_from_look_angles(orbit_camera.free_yaw, orbit_camera.free_pitch);
    let right = forward.cross(Vec3::Y).normalize_or_zero();

    let mut movement = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        movement += forward;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        movement -= forward;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        movement += right;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        movement -= right;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        movement += Vec3::Y;
    }
    if keyboard.pressed(KeyCode::KeyQ) {
        movement -= Vec3::Y;
    }

    let movement = movement.normalize_or_zero();
    if movement == Vec3::ZERO {
        return;
    }

    // Auto-scale speed with distance to the nearest body: slow for close-up
    // inspection, fast across interplanetary gaps.
    let nearest = nearest_body_distance(&body_runtime, orbit_camera.free_position);
    let boost = if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
        FREE_CAMERA_BOOST_MULTIPLIER
    } else {
        1.0
    };
    let speed = (nearest * FREE_CAMERA_SPEED_FACTOR)
        .clamp(FREE_CAMERA_MIN_SPEED, FREE_CAMERA_MAX_SPEED)
        * boost;

    orbit_camera.free_position += movement * speed * time.delta_secs();
}

pub(super) fn track_selected_body(
    time: Res<Time>,
    simulation_state: Res<SimulationState>,
    body_runtime: Res<BodyRuntime>,
    mut orbit_camera: ResMut<OrbitCameraState>,
) {
    // Keep the selected body centered even while simulation time advances.
    if orbit_camera.mode != CameraMode::Orbit {
        return;
    }
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
    if orbit_camera.mode != CameraMode::Orbit {
        return;
    }

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
    let at_lerp = 1.0 - (-4.0 * dt).exp();
    let dist_lerp = 1.0 - (-3.0 * dt).exp();

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

    let (translation, look_at_point) = match orbit_camera.mode {
        CameraMode::Orbit => (
            orbit_camera_world_position(&orbit_camera),
            orbit_camera.target,
        ),
        CameraMode::Free => {
            let forward =
                direction_from_look_angles(orbit_camera.free_yaw, orbit_camera.free_pitch);
            (
                orbit_camera.free_position,
                orbit_camera.free_position + forward,
            )
        }
    };

    *transform = Transform::from_translation(translation).looking_at(look_at_point, Vec3::Y);
}

fn compute_target_distance_for_body(visual_radius: f32) -> f32 {
    (visual_radius * 12.0).clamp(0.25, 120.0)
}

/// World-space position of the orbit camera from its spherical parameters.
fn orbit_camera_world_position(orbit_camera: &OrbitCameraState) -> Vec3 {
    let x = orbit_camera.distance * orbit_camera.pitch.cos() * orbit_camera.yaw.sin();
    let y = orbit_camera.distance * orbit_camera.pitch.sin();
    let z = orbit_camera.distance * orbit_camera.pitch.cos() * orbit_camera.yaw.cos();
    orbit_camera.target + Vec3::new(x, y, z)
}

/// Unit look direction from yaw/pitch (matches the orbit camera's angle
/// convention so handoffs between the two cameras don't rotate the view).
fn direction_from_look_angles(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    )
}

/// Inverse of `direction_from_look_angles`: yaw/pitch for a given direction.
fn look_angles_from_direction(dir: Vec3) -> (f32, f32) {
    if dir.length_squared() < 1e-12 {
        return (0.0, 0.0);
    }
    let yaw = dir.x.atan2(dir.z);
    let pitch = dir.y.clamp(-1.0, 1.0).asin();
    (yaw, pitch)
}

/// Index of the body whose scene position is closest to `position`, if any.
fn nearest_body_index(body_runtime: &BodyRuntime, position: Vec3) -> Option<usize> {
    body_runtime
        .positions
        .iter()
        .enumerate()
        .map(|(index, p)| (index, p.as_vec3().distance_squared(position)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// Distance from `position` to the nearest body center (0 if there are none).
fn nearest_body_distance(body_runtime: &BodyRuntime, position: Vec3) -> f32 {
    nearest_body_index(body_runtime, position)
        .map(|index| body_runtime.positions[index].as_vec3().distance(position))
        .unwrap_or(0.0)
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
    let lerp_factor = 1.0 - (-8.0 * delta_seconds).exp();
    current.lerp(desired, lerp_factor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::DVec3;

    #[test]
    fn compute_target_distance_for_body_clamps_to_expected_bounds() {
        assert_eq!(compute_target_distance_for_body(0.001), 0.25);
        assert_eq!(compute_target_distance_for_body(2.0), 24.0);
        assert_eq!(compute_target_distance_for_body(20.0), 120.0);
    }

    #[test]
    fn orient_camera_toward_sunward_keeps_pitch_in_valid_range() {
        let mut camera = OrbitCameraState {
            mode: CameraMode::Orbit,
            yaw: 0.0,
            pitch: 0.0,
            distance: 10.0,
            min_distance: 1.0,
            max_distance: 100.0,
            target: Vec3::ZERO,
            flight: None,
            free_position: Vec3::ZERO,
            free_yaw: 0.0,
            free_pitch: 0.0,
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

    #[test]
    fn look_angle_direction_round_trips() {
        for dir in [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 2.0, -3.0).normalize(),
            Vec3::new(-4.0, -1.0, 2.0).normalize(),
        ] {
            let dir = dir.normalize();
            let (yaw, pitch) = look_angles_from_direction(dir);
            let rebuilt = direction_from_look_angles(yaw, pitch);
            assert!(
                rebuilt.distance(dir) < 1e-5,
                "round trip failed: {dir:?} -> {rebuilt:?}"
            );
        }
    }

    #[test]
    fn nearest_body_index_picks_closest() {
        let body_runtime = BodyRuntime {
            positions: vec![
                DVec3::new(100.0, 0.0, 0.0),
                DVec3::new(5.0, 0.0, 0.0),
                DVec3::new(-50.0, 0.0, 0.0),
            ],
        };
        let nearest = nearest_body_index(&body_runtime, Vec3::new(6.0, 0.0, 0.0));
        assert_eq!(nearest, Some(1));
        let distance = nearest_body_distance(&body_runtime, Vec3::new(6.0, 0.0, 0.0));
        assert!((distance - 1.0).abs() < 1e-5);
    }

    #[test]
    fn nearest_body_index_is_none_when_empty() {
        let body_runtime = BodyRuntime {
            positions: Vec::new(),
        };
        assert_eq!(nearest_body_index(&body_runtime, Vec3::ZERO), None);
        assert_eq!(nearest_body_distance(&body_runtime, Vec3::ZERO), 0.0);
    }
}
