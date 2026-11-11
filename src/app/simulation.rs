use super::asteroids::AsteroidEntity;
use super::materials::PlanetRingMaterial;
use super::types::{
    AU_TO_SCENE_UNITS, AtmosphereLayer, AtmosphereOf, BODIES, BodyEntity, BodyRuntime, BodySpec,
    BodyTrails, CLOUD_SUPERROTATION_RADIANS_PER_SECOND, CameraMode, CloudLayer, CloudOf,
    EphemerisResource, HorizonsSyncState, MAX_SIMULATION_RATE_MULTIPLIER,
    MIN_SIMULATION_RATE_MULTIPLIER, OrbitCameraState, PlanetRing, RenderSettings, RingOf,
    SECONDS_PER_DAY, SimulationState, SpinModel, WorldPosition, drawn_size_factor, pole_rotation,
};
use super::util::eclipj2000_to_scene;
use crate::ephemeris::{
    CALLISTO_ORBIT, CHARON_ORBIT, EUROPA_ORBIT, GANYMEDE_ORBIT, IO_ORBIT, SatelliteOrbit,
    earth_rotation_angle_radians, iau_prime_meridian_radians, satellite_offset_au,
};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

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
            transform.rotation = body_rotation(&spec, unix_days, scene_position, &scene_positions);
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
    let pluto_pole = BODIES[pluto_index].pole_scene().as_vec3();
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

/// A body's orientation on `unix_days`. It is a function of the absolute date,
/// not an accumulation of per-frame steps, so a date jump or Backspace turns the
/// globe along with the clock, and a date shows the same face however the clock
/// got there and whenever the app was launched.
///
/// The mesh's local +Z stands on the pole with `pole_rotation`. Bevy's UV sphere
/// has u = 0 on local +X and u growing right-handed about +Z, so a map's centre
/// (u = 0.5) is half a turn from +X: the turn below carries that centre to its
/// longitude, measured from the prime meridian at `W`. A `south_up` map is a
/// half turn about the same axis, which swaps its poles and reverses its
/// longitudes without mirroring the surface.
fn body_rotation(
    spec: &BodySpec,
    unix_days: f64,
    own_position: DVec3,
    scene_positions: &[DVec3],
) -> Quat {
    let turn = prime_meridian_angle_radians(spec, unix_days, own_position, scene_positions)
        + spec.map.centre_longitude_deg.to_radians()
        + PI;
    let rotation = pole_rotation(spec) * Quat::from_rotation_z(turn.rem_euclid(TAU) as f32);
    if spec.map.south_up {
        rotation * Quat::from_rotation_x(std::f32::consts::PI)
    } else {
        rotation
    }
}

/// The angle `W` of `SpinModel`: from the node of the body's equator to its
/// prime meridian, right-handed about its pole.
fn prime_meridian_angle_radians(
    spec: &BodySpec,
    unix_days: f64,
    own_position: DVec3,
    scene_positions: &[DVec3],
) -> f64 {
    match spec.spin {
        SpinModel::Iau {
            w0_deg,
            deg_per_day,
        } => iau_prime_meridian_radians(w0_deg, deg_per_day, unix_days),
        // The rotation angle runs from the equinox, a quarter turn before the
        // node `W` starts from (Earth's pole is listed at right ascension 0).
        SpinModel::EarthRotationAngle => earth_rotation_angle_radians(unix_days) - FRAC_PI_2,
        SpinModel::LockedTo(primary) => {
            let toward_primary = body_index_for_target(primary)
                .map_or(DVec3::ZERO, |index| scene_positions[index] - own_position);
            let node = spec.equator_node_scene();
            let quarter_turn_on = spec.pole_scene().cross(node);
            toward_primary
                .dot(quarter_turn_on)
                .atan2(toward_primary.dot(node))
        }
    }
}

/// Rotation angle about a local +Z at `simulation_seconds` from launch, reduced
/// to one turn, for what has no surface longitudes to place: Venus's cloud shell
/// and the asteroid swarm. A positive rate is right-handed about +Z. The product
/// is taken in f64: f32 would lose whole turns over a long run at high speed.
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
    render_settings: Res<RenderSettings>,
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
        // The umbra is as wide as the planet is drawn.
        let spec = &BODIES[ring.index];
        material.params.z =
            spec.visual_radius * drawn_size_factor(spec, render_settings.true_scale);
    }
}

/// Resizes the bodies when the true-scale toggle flips. Meshes stay built at
/// `visual_radius` and only `Transform::scale` changes, multiplied by the ratio
/// of new to old size factor: that leaves Voyager's model scale alone, and the
/// atmosphere, cloud and ring entities resize with their body. Asteroids shrink
/// with Ceres so the belt never dwarfs it. The orbit camera's distance scales
/// with its body, which stays the same size on screen while its moons move out
/// to their real distance in planet radii (or back in).
#[allow(clippy::type_complexity)]
pub(super) fn apply_body_scale(
    render_settings: Res<RenderSettings>,
    mut applied_true_scale: Local<bool>,
    simulation_state: Res<SimulationState>,
    mut orbit_camera: ResMut<OrbitCameraState>,
    mut layers: Query<
        (
            &mut Transform,
            AnyOf<(&BodyEntity, &AtmosphereOf, &CloudOf, &RingOf)>,
        ),
        Without<AsteroidEntity>,
    >,
    mut asteroids: Query<&mut Transform, With<AsteroidEntity>>,
) {
    let true_scale = render_settings.true_scale;
    let previous = *applied_true_scale;
    if true_scale == previous {
        return;
    }
    *applied_true_scale = true_scale;
    let ratio =
        |spec: &BodySpec| drawn_size_factor(spec, true_scale) / drawn_size_factor(spec, previous);

    for (mut transform, (body, atmosphere, cloud, ring)) in &mut layers {
        let index = body
            .map(|b| b.index)
            .or(atmosphere.map(|a| a.index))
            .or(cloud.map(|c| c.index))
            .or(ring.map(|r| r.index));
        if let Some(index) = index {
            transform.scale *= ratio(&BODIES[index]);
        }
    }

    if let Some(ceres) = BODIES.iter().find(|spec| spec.display_name == "Ceres") {
        let ceres_ratio = ratio(ceres);
        for mut transform in &mut asteroids {
            transform.scale *= ceres_ratio;
        }
    }

    if orbit_camera.mode == CameraMode::Orbit
        && let Some(index) = simulation_state.selected_body_index
    {
        orbit_camera.distance *= ratio(&BODIES[index]);
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{BodySpec, KM_PER_AU, SpinModel, pole_rotation};
    use super::{
        CHARON_TO_PLUTO_MASS_RATIO, apply_jupiter_moon_positions,
        apply_pluto_charon_center_positions, body_index_for_target, body_rotation,
        charon_relative_scene_offset, position_is_reconstructed, prime_meridian_angle_radians,
        spin_angle_radians,
    };
    use crate::ephemeris::{CALLISTO_ORBIT, CHARON_ORBIT, IO_ORBIT};
    use bevy::math::DVec3;
    use bevy::prelude::*;
    use chrono::{TimeZone, Utc};
    use std::f64::consts::TAU;

    #[test]
    fn spin_angle_radians_keeps_the_rate_sign_within_one_turn() {
        assert!((spin_angle_radians(0.5, 2.0) - 1.0).abs() < 1e-6);
        assert!((f64::from(spin_angle_radians(-0.5, 2.0)) - (TAU - 1.0)).abs() < 1e-6);
        assert_eq!(spin_angle_radians(0.5, 0.0), 0.0);
    }

    #[test]
    fn spin_angle_radians_is_exact_after_centuries() {
        // Earth's rate, 400 years on: ~10^6 rad. Reduced in f64 the phase is
        // still good to a small fraction of a degree, where f32 arithmetic would
        // have lost whole turns.
        let rate = (TAU / 86_164.1) as f32;
        let seconds = 400.0 * 365.25 * super::SECONDS_PER_DAY;
        let exact = (f64::from(rate) * seconds).rem_euclid(TAU);
        let actual = f64::from(spin_angle_radians(rate, seconds));
        assert!((actual - exact).abs() < 1e-5);
    }

    fn body(name: &str) -> &'static BodySpec {
        super::BODIES
            .iter()
            .find(|b| b.display_name == name)
            .unwrap()
    }

    /// Where Bevy's UV sphere draws a map location: u = 0 on local +X with u
    /// growing right-handed about +Z, and v = 0 at +Z.
    fn mesh_point(u: f64, v: f64) -> Vec3 {
        let (angle, latitude) = (TAU * u, (0.5 - v) * std::f64::consts::PI);
        DVec3::new(
            latitude.cos() * angle.cos(),
            latitude.cos() * angle.sin(),
            latitude.sin(),
        )
        .as_vec3()
    }

    /// Orients a body's `Transform` exactly as `update_body_positions` does
    /// and returns a surface point at day 0 and just over an hour later.
    fn surface_point_before_and_after_spin(spec: &BodySpec) -> (Vec3, Vec3, Vec3) {
        let pole = spec.pole_scene().as_vec3();
        // A point on the mesh's equator.
        let local = Vec3::X;
        let at = |days: f64| body_rotation(spec, days, DVec3::ZERO, &[]) * local;
        (pole, at(0.0), at(0.05))
    }

    /// Scene direction from Earth to the Sun, from the Astronomical Almanac's
    /// low-precision solar longitude (good to ~0.01°), independent of the app.
    fn almanac_sun_direction(unix_days: f64) -> Vec3 {
        let days_from_j2000 = unix_days - 10_957.5;
        let mean_longitude = 280.460 + 0.985_647_4 * days_from_j2000;
        let mean_anomaly = (357.528 + 0.985_600_3 * days_from_j2000).to_radians();
        let longitude =
            (mean_longitude + 1.915 * mean_anomaly.sin() + 0.020 * (2.0 * mean_anomaly).sin())
                .to_radians();
        Vec3::new(longitude.cos() as f32, 0.0, -longitude.sin() as f32)
    }

    #[test]
    fn earth_map_faces_the_sun_at_the_real_subsolar_point() {
        // Expected subsolar points follow from the published equation of time
        // and solar declination for each date.
        let cases = [
            // 01:00 UTC, equation of time +16.4 min: over the western Pacific.
            ((2026, 11, 2, 1), -14.9_f32, 160.9_f32),
            // Noon UTC at the June solstice, equation of time -1.7 min: the
            // Tropic of Cancer on the Greenwich meridian.
            ((2026, 6, 21, 12), 23.44, 0.4),
        ];
        let earth = body("Earth");
        for ((year, month, day, hour), latitude, longitude) in cases {
            let unix_days = Utc
                .with_ymd_and_hms(year, month, day, hour, 0, 0)
                .unwrap()
                .timestamp() as f64
                / super::SECONDS_PER_DAY;
            let (latitude, longitude) = (latitude.to_radians(), longitude.to_radians());
            // Where a UV sphere draws that map location: the map's Greenwich
            // (u = 0.5) on local -X, longitude growing east, north on +Z.
            let surface_point = Vec3::new(
                -latitude.cos() * longitude.cos(),
                -latitude.cos() * longitude.sin(),
                latitude.sin(),
            );
            let rotation = body_rotation(earth, unix_days, DVec3::ZERO, &[]);
            let error_degrees = (rotation * surface_point)
                .angle_between(almanac_sun_direction(unix_days))
                .to_degrees();
            assert!(
                error_degrees < 1.0,
                "{year}-{month}-{day} {hour}:00 UTC: map is {error_degrees}° from the subsolar point"
            );
        }
    }

    /// Against CSPICE itself: prime meridian and north pole in scene space, from
    /// `pxform("ECLIPJ2000", "IAU_<BODY>", et)` with the committed kernels. Stored
    /// as data because CSPICE allows one lock per process, which a live test would
    /// contend for with the kernel test in `ephemeris.rs`, and so the portable
    /// build checks them too. Earth is left out: `IAU_EARTH` is a low-precision
    /// model and the rotation-angle tests are stricter. The tolerance allows for
    /// the kernel's periodic terms, which `SpinModel::Iau` leaves out.
    #[test]
    fn body_frames_match_cspice() {
        #[rustfmt::skip]
        let cases: [(&str, f64, [f64; 3], [f64; 3]); 20] = [
            ("Sun", 20759.041666666668, [-0.969816, 0.126146, -0.208673], [0.122353, 0.992001, 0.031038]),
            ("Mercury", 20759.041666666668, [0.893849, -0.045546, -0.446049], [0.091310, 0.992471, 0.081635]),
            ("Venus", 20759.041666666668, [0.730694, -0.021081, -0.682380], [0.018691, 0.999766, -0.010873]),
            ("Mars", 20759.041666666668, [0.441039, -0.165034, -0.882184], [0.446123, 0.893222, 0.055936]),
            ("Ceres", 20759.041666666668, [-0.502192, 0.048226, -0.863410], [0.144066, 0.989156, -0.028545]),
            ("Vesta", 20759.041666666668, [0.280711, 0.137643, -0.949871], [0.466256, 0.845485, 0.260308]),
            ("Jupiter", 20759.041666666668, [-0.932036, -0.000633, -0.362365], [-0.014593, 0.999253, 0.035790]),
            ("Saturn", 20759.041666666668, [-0.933080, 0.233676, 0.273420], [0.085505, 0.882519, -0.462439]),
            ("Uranus", 20759.041666666668, [-0.080917, -0.989516, 0.119630], [-0.212000, 0.134363, 0.967989]),
            ("Neptune", 20759.041666666668, [0.759779, -0.081676, -0.645031], [0.360411, 0.878614, 0.313274]),
            ("Sun", 47647.5, [0.806775, -0.117625, 0.579032], [0.122353, 0.992001, 0.031038]),
            ("Mercury", 47647.5, [-0.928282, 0.054939, 0.367798], [0.091122, 0.992480, 0.081732]),
            ("Venus", 47647.5, [-0.988378, 0.016835, -0.151083], [0.018691, 0.999766, -0.010873]),
            ("Mars", 47647.5, [-0.511161, 0.306563, -0.802953], [0.446017, 0.893202, 0.057084]),
            ("Ceres", 47647.5, [0.372966, -0.027557, 0.927436], [0.144066, 0.989156, -0.028545]),
            ("Vesta", 47647.5, [0.826206, -0.310998, -0.469749], [0.466256, 0.845485, 0.260308]),
            ("Jupiter", 47647.5, [0.863544, -0.005399, 0.504244], [-0.014647, 0.999252, 0.035783]),
            ("Saturn", 47647.5, [0.746921, 0.250342, 0.615986], [0.085578, 0.882516, -0.462431]),
            ("Uranus", 47647.5, [0.975951, 0.080549, 0.202563], [-0.212000, 0.134363, 0.967989]),
            ("Neptune", 47647.5, [0.805669, -0.463074, 0.369405], [0.363762, 0.878941, 0.308448]),
        ];
        for (name, unix_days, prime_meridian, pole) in cases {
            let spec = body(name);
            let rotation = body_rotation(spec, unix_days, DVec3::ZERO, &[]);
            let north = if spec.map.south_up { -Vec3::Z } else { Vec3::Z };
            let u = 0.5 - spec.map.centre_longitude_deg / 360.0;
            let meridian_error = (rotation * mesh_point(u, 0.5))
                .angle_between(DVec3::from_array(prime_meridian).as_vec3())
                .to_degrees();
            let pole_error = (rotation * north)
                .angle_between(DVec3::from_array(pole).as_vec3())
                .to_degrees();
            assert!(
                meridian_error < 1.0 && pole_error < 1.0,
                "{name} on {unix_days}: meridian {meridian_error}°, pole {pole_error}°"
            );
        }
    }

    /// Known surface features at the map position (u, v) where each texture
    /// shows them, read from the maps themselves, with their published latitude
    /// and east longitude. The positions were read by eye, so the tolerance only
    /// catches a wrong map centre or orientation.
    #[test]
    fn map_features_land_at_their_published_coordinates() {
        let pixel = |x: f64, y: f64| (x / 700.0, y / 350.0);
        let features = [
            ("Mercury", "Kuiper", pixel(289.0, 198.0), -11.3, -31.5),
            ("Venus", "Maxwell Montes", pixel(345.0, 300.0), 65.2, 3.3),
            ("Moon", "Mare Crisium", pixel(465.0, 140.0), 17.0, 59.0),
            ("Mars", "Olympus Mons", pixel(90.0, 138.0), 18.65, -133.8),
            ("Ceres", "Occator", pixel(467.0, 135.0), 19.8, 239.3),
            ("Io", "Pele", pixel(550.0, 210.0), -18.7, 104.5),
            ("Europa", "Pwyll", pixel(172.0, 222.0), -25.2, 88.9),
            ("Ganymede", "Tros", pixel(645.0, 150.0), 11.0, 332.4),
            ("Callisto", "Valhalla", pixel(583.0, 156.0), 15.0, 304.0),
            (
                "Pluto",
                "Sputnik Planitia",
                pixel(345.0, 125.0),
                25.0,
                177.0,
            ),
        ];
        let positions: Vec<DVec3> = (0..super::BODIES.len())
            .map(|i| DVec3::new(10.0 * i as f64, 3.0, -7.0 * i as f64))
            .collect();
        for (name, feature, (u, v), latitude, longitude) in features {
            let spec = body(name);
            let own = positions[body_index_for_target(spec.spice_target).unwrap()];
            let unix_days = 20_000.25;
            let drawn = body_rotation(spec, unix_days, own, &positions) * mesh_point(u, v);
            // The same place from the body's own frame: +X on the prime meridian
            // and +Z on the IAU pole, independent of how the map is laid out.
            let (lat, lon) = (f64::to_radians(latitude), f64::to_radians(longitude));
            let w = prime_meridian_angle_radians(spec, unix_days, own, &positions);
            let expected = pole_rotation(spec)
                * Quat::from_rotation_z(w as f32)
                * DVec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()).as_vec3();
            let error = drawn.angle_between(expected).to_degrees();
            assert!(error < 10.0, "{name}: {feature} is {error}° off");
        }
    }

    #[test]
    fn tidally_locked_bodies_turn_longitude_zero_to_their_primary() {
        for (body_index, spec) in super::BODIES.iter().enumerate() {
            let SpinModel::LockedTo(primary) = spec.spin else {
                continue;
            };
            let mut positions = vec![DVec3::ZERO; super::BODIES.len()];
            let primary_index = body_index_for_target(primary).unwrap();
            positions[primary_index] = DVec3::new(5.0, -2.0, 9.0);
            // Around a whole orbit in the body's equatorial plane.
            for step in 0..8 {
                let angle = TAU * f64::from(step) / 8.0;
                let node = spec.equator_node_scene();
                let toward_primary =
                    node * angle.cos() + spec.pole_scene().cross(node) * angle.sin();
                let own = positions[primary_index] - toward_primary * 3.0;
                positions[body_index] = own;
                // Longitude 0 on the map: the centre, or the seam of a map
                // centred on 180°.
                let u = 0.5 - spec.map.centre_longitude_deg / 360.0;
                let near_side = body_rotation(spec, 0.0, own, &positions) * mesh_point(u, 0.5);
                let error = near_side
                    .angle_between(toward_primary.as_vec3())
                    .to_degrees();
                assert!(error < 0.1, "{} misses by {error}°", spec.display_name);
            }
        }
    }

    #[test]
    fn prograde_bodies_spin_right_handed_about_their_pole() {
        // Right-handed about `pole`: the surface moves along `pole x r`, the
        // same sense every orbit runs in (counter-clockwise seen from north).
        for name in ["Earth", "Mars", "Jupiter", "Saturn", "Neptune"] {
            let spec = body(name);
            let (pole, before, after) = surface_point_before_and_after_spin(spec);
            assert!(
                (after - before).dot(pole.cross(before)) > 0.0,
                "{name} should spin right-handed about its pole"
            );
        }
    }

    #[test]
    fn retrograde_bodies_spin_left_handed_about_their_iau_pole() {
        for name in ["Venus", "Uranus"] {
            let spec = body(name);
            let (pole, before, after) = surface_point_before_and_after_spin(spec);
            assert!(
                (after - before).dot(pole.cross(before)) < 0.0,
                "{name} should spin left-handed about its pole"
            );
        }
    }

    #[test]
    fn charon_orbits_in_the_same_sense_as_pluto_spins() {
        // Tidally locked: Charon's orbital motion and Pluto's spin share a
        // sense about the shared pole.
        let pole = body("Pluto").pole_scene().as_vec3();
        let before = charon_relative_scene_offset(100.0, 250.0).as_vec3();
        let after =
            charon_relative_scene_offset(100.0 + 0.01 * CHARON_ORBIT.period_days, 250.0).as_vec3();
        assert!(
            (after - before).dot(pole.cross(before)) > 0.0,
            "Charon should orbit right-handed about Pluto's pole"
        );
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
