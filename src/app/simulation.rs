use super::materials::PlanetRingMaterial;
use super::types::{
    AU_TO_SCENE_UNITS, AtmosphereLayer, AtmosphereOf, BODIES, BodyEntity, BodyRuntime, BodyTrails,
    CLOUD_SUPERROTATION_RADIANS_PER_SECOND, CameraMode, CloudLayer, CloudOf, EphemerisResource,
    HorizonsSyncState, MAX_SIMULATION_RATE_MULTIPLIER, MIN_SIMULATION_RATE_MULTIPLIER,
    OrbitCameraState, PlanetRing, RingOf, SECONDS_PER_DAY, SimulationState, WorldPosition,
    pole_rotation,
};
use super::util::eclipj2000_to_scene;
use crate::ephemeris::{
    CALLISTO_ORBIT, CHARON_ORBIT, EUROPA_ORBIT, GANYMEDE_ORBIT, IO_ORBIT, SatelliteOrbit,
    satellite_offset_au,
};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use std::f64::consts::TAU;

const CHARON_TO_PLUTO_MASS_RATIO: f64 = 0.1218;

/// Targets whose scene position is rebuilt from a primary body *after* the
/// ephemeris pass in `update_body_positions`: `apply_jupiter_moon_positions`
/// and `apply_pluto_charon_center_positions` overwrite these entries outright.
/// Any Horizons offset fetched for them is therefore discarded before it can
/// reach a transform, which is why the sync skips them entirely — see
/// `setup::horizons_sync_skips_target`.
pub(super) const RECONSTRUCTED_TARGETS: [&str; 5] =
    ["IO", "EUROPA", "GANYMEDE", "CALLISTO", "CHARON"];

/// True when `spice_target`'s position is reconstructed from a primary rather
/// than taken from the ephemeris, making a Horizons correction meaningless.
pub(super) fn position_is_reconstructed(spice_target: &str) -> bool {
    RECONSTRUCTED_TARGETS
        .iter()
        .any(|target| target.eq_ignore_ascii_case(spice_target))
}

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
        orbit_camera.mode = CameraMode::Orbit;
        orbit_camera.flight = None;
        orbit_camera.target = DVec3::ZERO;
        orbit_camera.pan_offset = DVec3::ZERO;
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
    simulation_state: Res<SimulationState>,
    ephemeris: NonSend<EphemerisResource>,
    horizons_sync: Res<HorizonsSyncState>,
    mut body_runtime: ResMut<BodyRuntime>,
    mut body_query: Query<(&BodyEntity, &mut Transform, &mut WorldPosition)>,
) {
    let au_to_scene_units = AU_TO_SCENE_UNITS;
    let elapsed_seconds = simulation_state.elapsed_simulation_days * SECONDS_PER_DAY;
    let mut scene_positions = vec![DVec3::ZERO; BODIES.len()];

    for body_index in 0..BODIES.len() {
        let spec = BODIES[body_index];
        // The reconstructed satellites are rebuilt from their primary below;
        // the ephemeris has nothing to say about them.
        if position_is_reconstructed(spec.spice_target) {
            continue;
        }
        let position_au = ephemeris
            .ephemeris
            .position_au(spec.spice_target, simulation_state.elapsed_simulation_days);

        scene_positions[body_index] = eclipj2000_to_scene(position_au, au_to_scene_units);

        if horizons_sync.enabled
            && let Some(offset_au) = horizons_sync.per_body_au_offset.get(body_index)
        {
            scene_positions[body_index] += *offset_au * au_to_scene_units;
        }
    }

    // Satellite phases run on the same absolute clock as every analytic
    // position, so a date jump moves the moons along with their primary.
    let unix_days = ephemeris
        .ephemeris
        .unix_days_at(simulation_state.elapsed_simulation_days);
    apply_pluto_charon_center_positions(&mut scene_positions, unix_days, au_to_scene_units);
    apply_jupiter_moon_positions(&mut scene_positions, unix_days, au_to_scene_units);

    for (body, mut transform, mut world_position) in &mut body_query {
        let spec = BODIES[body.index];
        let scene_position = scene_positions[body.index];

        world_position.0 = scene_position;
        if spec.model_file.is_some() {
            // Spacecraft hold attitude rather than spin: point the model's +Y
            // axis (Voyager's high-gain dish) back at the Sun — Earth, at
            // these distances.
            if let Some(sunward) = (-scene_position.as_vec3()).try_normalize() {
                transform.rotation = Quat::from_rotation_arc(Vec3::Y, sunward);
            }
        } else {
            // Rotation phase is a function of simulation time, not an
            // accumulation of per-frame steps, so a date jump or Backspace
            // turns the globe along with the clock and the result is the
            // same however the clock got there. Local +Z is the body's
            // `pole_direction`; a positive angle is right-handed about it.
            transform.rotation = pole_rotation(&spec)
                * Quat::from_rotation_z(spin_angle_radians(
                    spec.spin_radians_per_second,
                    elapsed_seconds,
                ));
        }

        if let Some(slot) = body_runtime.positions.get_mut(body.index) {
            *slot = scene_position;
        }
    }
}

fn body_index_for_target(target: &str) -> Option<usize> {
    BODIES.iter().position(|spec| spec.spice_target == target)
}

/// Scene-space offset of a satellite from its primary on `unix_days`: the
/// ephemeris's single satellite rule, mapped like every other position.
fn satellite_scene_offset(orbit: &SatelliteOrbit, unix_days: f64, au_to_scene_units: f64) -> DVec3 {
    eclipj2000_to_scene(satellite_offset_au(orbit, unix_days), au_to_scene_units)
}

fn charon_relative_scene_offset(unix_days: f64, au_to_scene_units: f64) -> DVec3 {
    let analytic = satellite_scene_offset(&CHARON_ORBIT, unix_days, au_to_scene_units);

    // Tilt the orbit so its normal aligns with Pluto's spin pole instead of the
    // ecliptic Y axis. Pluto and Charon are mutually tidally locked, so Charon
    // orbits in Pluto's equatorial plane (~120° inclined to the ecliptic).
    let Some(pluto_index) = body_index_for_target("PLUTO BARYCENTER") else {
        return analytic;
    };
    let pluto_pole = Vec3::from_array(BODIES[pluto_index].pole_direction).normalize();
    let tilt = Quat::from_rotation_arc(Vec3::Y, pluto_pole);
    tilt.mul_vec3(analytic.as_vec3()).as_dvec3()
}

/// Galilean moons, with the orbit each is placed on relative to Jupiter.
const JUPITER_MOON_ORBITS: [(&str, &SatelliteOrbit); 4] = [
    ("IO", &IO_ORBIT),
    ("EUROPA", &EUROPA_ORBIT),
    ("GANYMEDE", &GANYMEDE_ORBIT),
    ("CALLISTO", &CALLISTO_ORBIT),
];

fn apply_jupiter_moon_positions(
    scene_positions: &mut [DVec3],
    unix_days: f64,
    au_to_scene_units: f64,
) {
    let Some(jupiter_index) = body_index_for_target("JUPITER BARYCENTER") else {
        return;
    };
    let jupiter_pos = scene_positions[jupiter_index];

    for (moon_target, orbit) in &JUPITER_MOON_ORBITS {
        let Some(moon_index) = body_index_for_target(moon_target) else {
            continue;
        };
        scene_positions[moon_index] =
            jupiter_pos + satellite_scene_offset(orbit, unix_days, au_to_scene_units);
    }
}

fn apply_pluto_charon_center_positions(
    scene_positions: &mut [DVec3],
    unix_days: f64,
    au_to_scene_units: f64,
) {
    let Some(pluto_barycenter_index) = body_index_for_target("PLUTO BARYCENTER") else {
        return;
    };
    let Some(charon_index) = body_index_for_target("CHARON") else {
        return;
    };

    let pluto_charon_barycenter = scene_positions[pluto_barycenter_index];
    let charon_from_pluto = charon_relative_scene_offset(unix_days, au_to_scene_units);
    let charon_mass_fraction = CHARON_TO_PLUTO_MASS_RATIO / (1.0 + CHARON_TO_PLUTO_MASS_RATIO);
    let pluto_mass_fraction = 1.0 - charon_mass_fraction;

    // Reconstruct Pluto/Charon center positions from the Pluto-Charon barycenter so
    // separation remains on the same physical scale as all other AU-derived distances.
    scene_positions[pluto_barycenter_index] =
        pluto_charon_barycenter - charon_from_pluto * charon_mass_fraction;
    scene_positions[charon_index] =
        pluto_charon_barycenter + charon_from_pluto * pluto_mass_fraction;
}

/// Rotation angle about a body's local +Z (its `pole_direction`) at
/// `simulation_seconds` from launch, reduced to one turn. The sign is the
/// body's own: `BodySpec::spin_radians_per_second` is positive for
/// right-handed rotation about the pole, which for the planets (pole =
/// ecliptic north) is the prograde sense they orbit in, so an eastward-moving
/// surface and a counter-clockwise orbit seen from north go together. Venus
/// and Uranus carry negative rates; Pluto and Charon are positive about their
/// tilted shared pole, matching Charon's orbit. The product is taken in f64:
/// 400 years of Earth spin is ~10^6 rad, more than f32 keeps to a degree.
pub(super) fn spin_angle_radians(spin_radians_per_second: f32, simulation_seconds: f64) -> f32 {
    (f64::from(spin_radians_per_second) * simulation_seconds).rem_euclid(TAU) as f32
}

pub(super) fn sync_atmosphere_positions(
    body_runtime: Res<BodyRuntime>,
    mut atmosphere_query: Query<(&AtmosphereOf, &mut WorldPosition), With<AtmosphereLayer>>,
) {
    for (atmosphere, mut world_position) in &mut atmosphere_query {
        if let Some(&position) = body_runtime.positions.get(atmosphere.index) {
            world_position.0 = position;
        }
    }
}

/// Keeps each cloud shell centred on its parent body and spins it about the
/// body's pole at the super-rotation rate, so the clouds drift over the surface
/// map. Mirrors `sync_atmosphere_positions` but adds the independent rotation.
pub(super) fn sync_cloud_layers(
    simulation_state: Res<SimulationState>,
    body_runtime: Res<BodyRuntime>,
    mut cloud_query: Query<(&CloudOf, &mut Transform, &mut WorldPosition), With<CloudLayer>>,
) {
    let elapsed_seconds = simulation_state.elapsed_simulation_days * SECONDS_PER_DAY;
    let spin = Quat::from_rotation_z(spin_angle_radians(
        CLOUD_SUPERROTATION_RADIANS_PER_SECOND,
        elapsed_seconds,
    ));

    for (cloud, mut transform, mut world_position) in &mut cloud_query {
        if let Some(&position) = body_runtime.positions.get(cloud.index) {
            world_position.0 = position;
        }
        if let Some(spec) = BODIES.get(cloud.index) {
            transform.rotation = pole_rotation(spec) * spin;
        }
    }
}

pub(super) fn sync_ring_positions(
    body_runtime: Res<BodyRuntime>,
    mut ring_query: Query<(&RingOf, &mut WorldPosition), With<PlanetRing>>,
) {
    for (ring, mut world_position) in &mut ring_query {
        if let Some(&position) = body_runtime.positions.get(ring.index) {
            world_position.0 = position;
        }
    }
}

/// Pushes the parent planet's current world-space (heliocentric) position
/// into each ring material's `planet_position` uniform so the WGSL shader can
/// compute the cylindrical eclipse cast by the planet onto the ring disc.
pub(super) fn sync_ring_material_uniforms(
    body_runtime: Res<BodyRuntime>,
    mut ring_materials: ResMut<Assets<PlanetRingMaterial>>,
    ring_query: Query<(&RingOf, &MeshMaterial3d<PlanetRingMaterial>), With<PlanetRing>>,
) {
    for (ring, material_handle) in &ring_query {
        let Some(position) = body_runtime.positions.get(ring.index) else {
            continue;
        };
        let Some(mut material) = ring_materials.get_mut(&material_handle.0) else {
            continue;
        };
        let p = position.as_vec3();
        material.planet_position = Vec4::new(p.x, p.y, p.z, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{BodySpec, KM_PER_AU, pole_rotation};
    use super::{
        CHARON_TO_PLUTO_MASS_RATIO, apply_jupiter_moon_positions,
        apply_pluto_charon_center_positions, body_index_for_target, charon_relative_scene_offset,
        position_is_reconstructed, spin_angle_radians,
    };
    use crate::ephemeris::{CALLISTO_ORBIT, CHARON_ORBIT, IO_ORBIT};
    use bevy::math::DVec3;
    use bevy::prelude::*;
    use std::f64::consts::TAU;

    #[test]
    fn spin_angle_radians_keeps_the_rate_sign_within_one_turn() {
        assert!((spin_angle_radians(0.5, 2.0) - 1.0).abs() < 1e-6);
        assert!((f64::from(spin_angle_radians(-0.5, 2.0)) - (TAU - 1.0)).abs() < 1e-6);
        assert_eq!(spin_angle_radians(0.5, 0.0), 0.0);
    }

    #[test]
    fn spin_angle_radians_is_exact_after_centuries() {
        // Earth, 400 years on: ~10^6 rad. Reduced in f64 the phase is still
        // good to a small fraction of a degree, where f32 arithmetic would
        // have lost whole turns.
        let earth = super::BODIES
            .iter()
            .find(|b| b.display_name == "Earth")
            .unwrap();
        let seconds = 400.0 * 365.25 * super::SECONDS_PER_DAY;
        let exact = (f64::from(earth.spin_radians_per_second) * seconds).rem_euclid(TAU);
        let actual = f64::from(spin_angle_radians(earth.spin_radians_per_second, seconds));
        assert!((actual - exact).abs() < 1e-5);
    }

    /// Orients a body's `Transform` exactly as `update_body_positions` does
    /// and returns a surface point at launch and one minute later.
    fn surface_point_before_and_after_spin(spec: &BodySpec) -> (Vec3, Vec3, Vec3) {
        let pole = Vec3::from_array(spec.pole_direction).normalize();
        // A point on the mesh's equator (Bevy's UV sphere has its poles on
        // local +-Z and its texture's north at +Z).
        let local = Vec3::X;
        let at = |seconds: f64| {
            (pole_rotation(spec)
                * Quat::from_rotation_z(spin_angle_radians(spec.spin_radians_per_second, seconds)))
                * local
        };
        (pole, at(0.0), at(60.0))
    }

    #[test]
    fn prograde_bodies_spin_right_handed_about_their_pole() {
        // Right-handed about `pole`: the surface moves along `pole x r`, the
        // same sense every orbit runs in (counter-clockwise seen from north).
        for name in ["Earth", "Mars", "Jupiter", "Pluto", "Charon"] {
            let spec = super::BODIES
                .iter()
                .find(|b| b.display_name == name)
                .unwrap();
            let (pole, before, after) = surface_point_before_and_after_spin(spec);
            assert!(
                (after - before).dot(pole.cross(before)) > 0.0,
                "{name} should spin right-handed about its pole"
            );
        }
    }

    #[test]
    fn retrograde_bodies_spin_left_handed_about_ecliptic_north() {
        for name in ["Venus", "Uranus"] {
            let spec = super::BODIES
                .iter()
                .find(|b| b.display_name == name)
                .unwrap();
            let (pole, before, after) = surface_point_before_and_after_spin(spec);
            assert!(
                (after - before).dot(pole.cross(before)) < 0.0,
                "{name} should spin left-handed about ecliptic north"
            );
        }
    }

    #[test]
    fn charon_orbits_in_the_same_sense_as_pluto_spins() {
        // Tidally locked: Charon's orbital motion and Pluto's spin share a
        // sense about the shared pole.
        let pluto = super::BODIES
            .iter()
            .find(|b| b.display_name == "Pluto")
            .unwrap();
        let (pole, _, _) = surface_point_before_and_after_spin(pluto);
        let before = charon_relative_scene_offset(100.0, 250.0).as_vec3();
        let after =
            charon_relative_scene_offset(100.0 + 0.01 * CHARON_ORBIT.period_days, 250.0).as_vec3();
        assert!(
            (after - before).dot(pole.cross(before)) > 0.0,
            "Charon should orbit right-handed about Pluto's pole"
        );
        assert!(pluto.spin_radians_per_second > 0.0);
    }

    #[test]
    fn spin_angle_radians_zero_rate_is_zero() {
        assert_eq!(spin_angle_radians(0.0, 2.0e9), 0.0);
    }

    #[test]
    fn apply_jupiter_moon_positions_places_io_at_correct_scene_distance() {
        let mut positions = vec![DVec3::ZERO; super::BODIES.len()];
        let jupiter_index =
            body_index_for_target("JUPITER BARYCENTER").expect("jupiter index should exist");
        let io_index = body_index_for_target("IO").expect("io index should exist");
        positions[jupiter_index] = DVec3::new(500.0, 0.0, 0.0);

        apply_jupiter_moon_positions(&mut positions, 0.0, 250.0);

        // Y is the inclination wobble; orbital radius is the X-Z magnitude.
        let offset = positions[io_index] - positions[jupiter_index];
        let xz_radius = (offset.x * offset.x + offset.z * offset.z).sqrt();
        let expected = (IO_ORBIT.semi_major_axis_km / KM_PER_AU) * 250.0;
        assert!((xz_radius - expected).abs() < 1e-9);
    }

    #[test]
    fn apply_jupiter_moon_positions_places_callisto_at_correct_scene_distance() {
        let mut positions = vec![DVec3::ZERO; super::BODIES.len()];
        let jupiter_index =
            body_index_for_target("JUPITER BARYCENTER").expect("jupiter index should exist");
        let callisto_index =
            body_index_for_target("CALLISTO").expect("callisto index should exist");
        positions[jupiter_index] = DVec3::new(-100.0, 50.0, 20.0);

        apply_jupiter_moon_positions(&mut positions, 50.0, 250.0);

        // Y is the inclination wobble; orbital radius is the X-Z magnitude.
        let offset = positions[callisto_index] - positions[jupiter_index];
        let xz_radius = (offset.x * offset.x + offset.z * offset.z).sqrt();
        let expected = (CALLISTO_ORBIT.semi_major_axis_km / KM_PER_AU) * 250.0;
        assert!((xz_radius - expected).abs() < 1e-9);
    }

    #[test]
    fn reconstructed_targets_covers_every_overwritten_body() {
        // Every body an applier overwrites must be listed, or the Horizons sync
        // would keep fetching an offset that `update_body_positions` discards.
        for (moon_target, _) in &super::JUPITER_MOON_ORBITS {
            assert!(
                position_is_reconstructed(moon_target),
                "{moon_target} is overwritten by apply_jupiter_moon_positions but missing from RECONSTRUCTED_TARGETS"
            );
        }
        assert!(
            position_is_reconstructed("CHARON"),
            "Charon is overwritten by apply_pluto_charon_center_positions but missing from RECONSTRUCTED_TARGETS"
        );

        // And every listed target must be a real body, so a rename cannot leave
        // a dead string silently excluding nothing.
        for target in super::RECONSTRUCTED_TARGETS {
            assert!(
                body_index_for_target(target).is_some(),
                "{target} is in RECONSTRUCTED_TARGETS but is not a BODIES entry"
            );
        }
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
