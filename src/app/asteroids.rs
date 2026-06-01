//! Asteroid belt: a procedurally generated swarm of small bodies between
//! Mars and Jupiter.
//!
//! Each asteroid follows a Keplerian ellipse around the Sun (heliocentric,
//! ecliptic frame). Positions are recomputed every frame from the simulation
//! time, the same way the named bodies are. To keep render cost bounded we
//! build a small set of distinct lumpy meshes (`MESH_VARIANT_COUNT`) and a
//! single shared rocky `StandardMaterial`; Bevy batches identical
//! `Handle<Mesh>` + `Handle<StandardMaterial>` pairs into instanced draws,
//! so thousands of entities boil down to a handful of draw calls.
//!
//! The orbital elements are deterministic for a given seed so the belt
//! looks the same across runs.

use super::types::{AU_TO_SCENE_UNITS, AppPaths, RenderSettings, SECONDS_PER_DAY, SimulationState};
use super::util::{eclipj2000_to_scene, random01};
use bevy::asset::RenderAssetUsages;
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use std::f64::consts::TAU;

/// Asset paths for the four hand-modelled asteroid variants. Resolved
/// relative to `AppPaths::assets_root`; if any are missing we fall back
/// to procedurally generated lumpy spheres so the belt still renders.
const ASTEROID_MODEL_PATHS: [&str; MESH_VARIANT_COUNT] = [
    "models/asteroids/rock_01.glb",
    "models/asteroids/rock_02.glb",
    "models/asteroids/rock_03.glb",
    "models/asteroids/rock_04.glb",
];

/// Number of asteroids spawned. ~3000 is a good middle ground: dense enough
/// to read as a belt, light enough to update every frame on the CPU.
const ASTEROID_COUNT: usize = 3_000;

/// Distinct base meshes. Each asteroid picks one and applies a random
/// non-uniform scale and rotation, so 4 shapes produce thousands of
/// visually distinct rocks while letting Bevy batch identical handles.
const MESH_VARIANT_COUNT: usize = 4;

/// Inner / outer edges of the main belt in AU. Loosely matches the real
/// 2.06–3.27 AU extent.
const INNER_AU: f64 = 2.1;
const OUTER_AU: f64 = 3.3;

/// Maximum eccentricity and inclination for a generated orbit. Real belt
/// asteroids cluster well below these bounds; generous caps add visual
/// variety without producing escapes or nonsensical geometry.
const MAX_ECCENTRICITY: f64 = 0.20;
const MAX_INCLINATION_RAD: f64 = 0.30; // ~17°

/// Visual radius of an "average" asteroid in scene units. The largest
/// real belt asteroid (Ceres, modelled as a named body) has a 15× visual
/// radius of 0.04, so 0.012 keeps these clearly subordinate while still
/// being visible at solar-system zoom.
const BASE_VISUAL_RADIUS: f32 = 0.012;

/// Fixed deterministic seed for orbital element generation. Change this
/// to reshuffle the belt without altering element ranges.
const SEED: u64 = 0xA57E_0A1D_BE17_5EED;

/// Orbital elements + per-asterod render parameters. Stored in a single
/// `Vec` indexed by `AsteroidEntity::index` so the per-frame update can
/// iterate without repeated component lookups.
#[derive(Clone, Copy)]
pub(super) struct AsteroidOrbit {
    /// Semi-major axis (AU).
    pub(super) a: f64,
    /// Eccentricity (0 = circle, < 1 = ellipse).
    pub(super) e: f64,
    /// Inclination relative to the ecliptic (radians).
    pub(super) i: f64,
    /// Longitude of ascending node (radians, ECLIPJ2000).
    pub(super) raan: f64,
    /// Argument of perihelion (radians).
    pub(super) arg_peri: f64,
    /// Mean anomaly at the simulation epoch (radians).
    pub(super) mean_anomaly_at_epoch: f64,
    /// Mean motion (radians / simulation second). Pre-baked from Kepler's
    /// third law assuming `GM_sun` so we don't recompute every frame.
    pub(super) mean_motion: f64,
    /// Spin rate around the asteroid's local Y axis (rad / sim second).
    pub(super) spin: f32,
}

#[derive(Resource)]
pub(super) struct AsteroidBelt {
    pub(super) orbits: Vec<AsteroidOrbit>,
}

#[derive(Component)]
pub(super) struct AsteroidEntity {
    pub(super) index: u32,
}

/// Marker for the asteroid root entity, used by the visibility toggle.
#[derive(Component)]
pub(super) struct AsteroidMarker;

/// Spawn the entire belt at startup. Runs after `setup_scene` so the
/// shared starfield/lighting infrastructure is already up.
pub(super) fn spawn_asteroid_belt(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    app_paths: Res<AppPaths>,
) {
    // Prefer the hand-modelled glTF rocks if they're on disk; fall back to
    // a procedural lumpy sphere per variant otherwise. The check is on
    // disk presence (not asset-load result) because Bevy's asset loader
    // is async and we need the handles synchronously here. Identical
    // mesh handles batch into instanced draws either way.
    let mesh_handles: Vec<Handle<Mesh>> = (0..MESH_VARIANT_COUNT)
        .map(|variant| {
            let relative = ASTEROID_MODEL_PATHS[variant];
            let on_disk = app_paths.assets_root.join(relative);
            if on_disk.is_file() {
                // glTF primitive sub-asset path. Blender exports a single
                // primitive per mesh, so `Mesh0/Primitive0` is the rock.
                let label = format!("{relative}#Mesh0/Primitive0");
                asset_server.load(&label)
            } else {
                warn!("asteroid model {relative} missing — falling back to procedural mesh");
                meshes.add(build_lumpy_asteroid_mesh(variant as u64))
            }
        })
        .collect();

    // Single shared material — keeps draws batched. A muted carbonaceous
    // grey covers most C-type asteroids; individual variation comes from
    // mesh shape + scale, not colour.
    let asteroid_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.38, 0.34),
        perceptual_roughness: 0.95,
        metallic: 0.02,
        reflectance: 0.05,
        ..default()
    });

    let mut rng_state = SEED;
    let mut orbits = Vec::with_capacity(ASTEROID_COUNT);

    for index in 0..ASTEROID_COUNT {
        // --- orbital elements -------------------------------------------------
        // Power-law-ish radial distribution biased slightly toward the
        // inner belt, where real density peaks.
        let r1 = random01(&mut rng_state) as f64;
        let a = INNER_AU + (OUTER_AU - INNER_AU) * r1.powf(0.85);
        let e = (random01(&mut rng_state) as f64).powi(2) * MAX_ECCENTRICITY;
        // Square biasing keeps most asteroids near the ecliptic with a
        // long tail of inclined ones — matches the observed distribution.
        let i = (random01(&mut rng_state) as f64).powi(2) * MAX_INCLINATION_RAD;
        let raan = (random01(&mut rng_state) as f64) * TAU;
        let arg_peri = (random01(&mut rng_state) as f64) * TAU;
        let mean_anomaly_at_epoch = (random01(&mut rng_state) as f64) * TAU;

        // Kepler's third law in solar-mass units: T(years) = a^1.5.
        let period_years = a.powf(1.5);
        let period_seconds = period_years * 365.25 * SECONDS_PER_DAY;
        let mean_motion = TAU / period_seconds;

        // Spin: a few hours to a couple days. Sign random for retrograde.
        let spin_hours = 4.0 + random01(&mut rng_state) * 60.0;
        let spin_sign = if random01(&mut rng_state) < 0.5 {
            -1.0
        } else {
            1.0
        };
        let spin = spin_sign * std::f32::consts::TAU / (spin_hours * 3600.0);

        orbits.push(AsteroidOrbit {
            a,
            e,
            i,
            raan,
            arg_peri,
            mean_anomaly_at_epoch,
            mean_motion,
            spin,
        });

        // --- render params ----------------------------------------------------
        // Pick one of the base meshes; non-uniform scale + tilt produce
        // unique-looking rocks from a handful of templates.
        let variant = (random01(&mut rng_state) * MESH_VARIANT_COUNT as f32).floor() as usize
            % MESH_VARIANT_COUNT;
        let size_jitter = 0.5 + random01(&mut rng_state) * 1.8; // 0.5×–2.3×
        let scale_x = BASE_VISUAL_RADIUS * size_jitter * (0.75 + random01(&mut rng_state) * 0.5);
        let scale_y = BASE_VISUAL_RADIUS * size_jitter * (0.75 + random01(&mut rng_state) * 0.5);
        let scale_z = BASE_VISUAL_RADIUS * size_jitter * (0.75 + random01(&mut rng_state) * 0.5);

        let initial_rotation = Quat::from_euler(
            EulerRot::XYZ,
            random01(&mut rng_state) * std::f32::consts::TAU,
            random01(&mut rng_state) * std::f32::consts::TAU,
            random01(&mut rng_state) * std::f32::consts::TAU,
        );

        commands.spawn((
            Mesh3d(mesh_handles[variant].clone()),
            MeshMaterial3d(asteroid_material.clone()),
            Transform {
                translation: Vec3::ZERO, // overwritten on first frame
                rotation: initial_rotation,
                scale: Vec3::new(scale_x, scale_y, scale_z),
            },
            AsteroidEntity {
                index: index as u32,
            },
            AsteroidMarker,
        ));
    }

    commands.insert_resource(AsteroidBelt { orbits });
}

/// Per-frame: re-evaluate every asteroid's heliocentric position from
/// `simulation_state.elapsed_simulation_days` and apply the spin delta.
pub(super) fn update_asteroid_positions(
    time: Res<Time>,
    simulation_state: Res<SimulationState>,
    belt: Option<Res<AsteroidBelt>>,
    render_settings: Res<RenderSettings>,
    mut query: Query<(&AsteroidEntity, &mut Transform)>,
) {
    let Some(belt) = belt else {
        return;
    };
    if !render_settings.asteroids_enabled {
        return;
    }

    let elapsed_seconds = simulation_state.elapsed_simulation_days * SECONDS_PER_DAY;
    let frame_simulation_seconds = if simulation_state.paused {
        0.0
    } else {
        time.delta_secs() * simulation_state.simulation_rate as f32
    };

    for (entity, mut transform) in &mut query {
        let orbit = &belt.orbits[entity.index as usize];
        let position_au = kepler_position(orbit, elapsed_seconds);
        let scene_pos = eclipj2000_to_scene(
            [position_au.x, position_au.y, position_au.z],
            AU_TO_SCENE_UNITS,
        );
        transform.translation =
            Vec3::new(scene_pos.x as f32, scene_pos.y as f32, scene_pos.z as f32);

        if frame_simulation_seconds != 0.0 {
            transform.rotate_local_y(orbit.spin * frame_simulation_seconds);
        }
    }
}

/// Toggle visibility on/off when the user flips `asteroids_enabled`.
/// Cheaper than despawning the swarm.
pub(super) fn sync_asteroid_visibility(
    render_settings: Res<RenderSettings>,
    mut query: Query<&mut Visibility, With<AsteroidMarker>>,
) {
    let target = if render_settings.asteroids_enabled {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut query {
        if *visibility != target {
            *visibility = target;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Kepler helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Heliocentric position in the ECLIPJ2000 frame (AU), from the orbital
/// elements at simulation time `t` (seconds since epoch).
fn kepler_position(orbit: &AsteroidOrbit, t_seconds: f64) -> DVec3 {
    let mean_anomaly = orbit.mean_anomaly_at_epoch + orbit.mean_motion * t_seconds;
    let eccentric_anomaly = solve_kepler(mean_anomaly, orbit.e);

    let cos_e = eccentric_anomaly.cos();
    let sin_e = eccentric_anomaly.sin();

    // Position in the orbital plane (perifocal frame).
    let x_perifocal = orbit.a * (cos_e - orbit.e);
    let y_perifocal = orbit.a * (1.0 - orbit.e * orbit.e).sqrt() * sin_e;

    // Rotate perifocal → ECLIPJ2000 via (ω, i, Ω) Euler angles.
    let (sin_w, cos_w) = orbit.arg_peri.sin_cos();
    let (sin_i, cos_i) = orbit.i.sin_cos();
    let (sin_o, cos_o) = orbit.raan.sin_cos();

    let x = (cos_o * cos_w - sin_o * sin_w * cos_i) * x_perifocal
        + (-cos_o * sin_w - sin_o * cos_w * cos_i) * y_perifocal;
    let y = (sin_o * cos_w + cos_o * sin_w * cos_i) * x_perifocal
        + (-sin_o * sin_w + cos_o * cos_w * cos_i) * y_perifocal;
    let z = (sin_w * sin_i) * x_perifocal + (cos_w * sin_i) * y_perifocal;

    DVec3::new(x, y, z)
}

/// Newton's-method solve of Kepler's equation `M = E - e·sin(E)`.
/// 5 iterations is more than enough for `e ≤ 0.2`.
fn solve_kepler(mean_anomaly: f64, eccentricity: f64) -> f64 {
    let m = mean_anomaly.rem_euclid(TAU);
    let mut e_anom = if eccentricity < 0.8 {
        m
    } else {
        std::f64::consts::PI
    };
    for _ in 0..5 {
        let f = e_anom - eccentricity * e_anom.sin() - m;
        let f_prime = 1.0 - eccentricity * e_anom.cos();
        e_anom -= f / f_prime;
    }
    e_anom
}

// ─────────────────────────────────────────────────────────────────────────────
// Procedural mesh
// ─────────────────────────────────────────────────────────────────────────────

/// Build a unit-radius UV sphere and displace each vertex along its
/// normal by a sum of three low-frequency sinusoids. The result has a
/// chunky, irregular silhouette suitable for an asteroid. `seed` shifts
/// the noise so each variant looks different.
fn build_lumpy_asteroid_mesh(seed: u64) -> Mesh {
    const H_SEGMENTS: u32 = 18;
    const V_SEGMENTS: u32 = 12;

    let h = H_SEGMENTS as usize;
    let v = V_SEGMENTS as usize;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((h + 1) * (v + 1));
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity((h + 1) * (v + 1));
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity((h + 1) * (v + 1));
    let mut indices: Vec<u32> = Vec::with_capacity(h * v * 6);

    // Three deterministic frequency/direction triples, perturbed by seed.
    let s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) as f32 * 1e-9;
    let phases = [
        Vec3::new(s.sin(), (s + 1.3).cos(), (s + 2.7).sin()) * 1.7,
        Vec3::new((s + 0.9).cos(), (s + 2.1).sin(), (s + 0.4).cos()) * 3.1,
        Vec3::new((s + 1.7).sin(), (s + 0.6).cos(), (s + 3.3).sin()) * 5.4,
    ];
    let amplitudes = [0.22, 0.10, 0.05];

    for j in 0..=v {
        let phi = std::f32::consts::PI * j as f32 / v as f32;
        let (sin_phi, cos_phi) = phi.sin_cos();
        for i in 0..=h {
            let theta = std::f32::consts::TAU * i as f32 / h as f32;
            let (sin_t, cos_t) = theta.sin_cos();

            let dir = Vec3::new(sin_phi * cos_t, cos_phi, sin_phi * sin_t);

            let mut displacement = 0.0_f32;
            for (phase, amp) in phases.iter().zip(amplitudes.iter()) {
                displacement +=
                    amp * ((dir.x * phase.x + dir.y * phase.y + dir.z * phase.z) * 1.5).sin();
            }
            let radius = (1.0 + displacement).max(0.55);

            let pos = dir * radius;
            positions.push([pos.x, pos.y, pos.z]);
            // Approximate normal: use the unperturbed direction. Lighting
            // is forgiving on rocks at this scale; recomputing analytical
            // normals would cost more than it adds visually.
            normals.push([dir.x, dir.y, dir.z]);
            uvs.push([i as f32 / h as f32, j as f32 / v as f32]);
        }
    }

    let stride = (h + 1) as u32;
    for j in 0..v as u32 {
        for i in 0..h as u32 {
            let i0 = j * stride + i;
            let i1 = i0 + 1;
            let i2 = i0 + stride;
            let i3 = i2 + 1;
            indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    // Recompute true normals so lighting reads the lumps. This is O(verts)
    // and only runs once per variant at startup, so it's cheap.
    mesh.compute_smooth_normals();
    mesh
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kepler_solver_converges_for_circular_orbit() {
        let e = solve_kepler(1.234, 0.0);
        // For e = 0, E should equal M (mod TAU).
        assert!((e - 1.234).abs() < 1e-9);
    }

    #[test]
    fn kepler_solver_converges_for_moderate_eccentricity() {
        for &e in &[0.05, 0.10, 0.20] {
            for k in 0..16 {
                let m = (k as f64) * (TAU / 16.0);
                let solved = solve_kepler(m, e);
                let residual = solved - e * solved.sin() - m.rem_euclid(TAU);
                assert!(
                    residual.abs() < 1e-8,
                    "residual {residual} too high at e={e}, m={m}"
                );
            }
        }
    }

    #[test]
    fn kepler_position_stays_within_orbit_bounds() {
        // A circular 2.7 AU orbit should stay near 2.7 AU at all times.
        let orbit = AsteroidOrbit {
            a: 2.7,
            e: 0.0,
            i: 0.0,
            raan: 0.0,
            arg_peri: 0.0,
            mean_anomaly_at_epoch: 0.0,
            mean_motion: TAU / (3.0 * 365.25 * SECONDS_PER_DAY),
            spin: 0.0,
        };
        for k in 0..32 {
            let t = k as f64 * SECONDS_PER_DAY * 30.0;
            let p = kepler_position(&orbit, t);
            let r = p.length();
            assert!((r - 2.7).abs() < 1e-9, "r={r} drifted from 2.7 AU");
            assert!(p.z.abs() < 1e-9, "circular ecliptic orbit should have z≈0");
        }
    }

    #[test]
    fn lumpy_mesh_has_expected_attributes() {
        let mesh = build_lumpy_asteroid_mesh(0);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
        assert!(mesh.indices().is_some());
    }

    #[test]
    fn distinct_seeds_produce_distinct_meshes() {
        let a = build_lumpy_asteroid_mesh(0);
        let b = build_lumpy_asteroid_mesh(1);
        let pa = a.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        let pb = b.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        // Different displacement → different vertex positions somewhere.
        assert_ne!(format!("{pa:?}"), format!("{pb:?}"));
    }
}
