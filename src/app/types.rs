use crate::ephemeris::SpiceEphemeris;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::tasks::Task;
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use std::collections::VecDeque;
use std::path::PathBuf;

pub(super) const TRAIL_MAX_POINTS: usize = 512;

pub(super) const AU_TO_SCENE_UNITS: f64 = 250.0;
pub(super) const KM_PER_AU: f64 = 149_597_870.7;
pub(super) const SECONDS_PER_DAY: f64 = 86_400.0;
pub(super) const DEFAULT_SIMULATION_RATE_MULTIPLIER: f64 = 1.0;
pub(super) const MIN_SIMULATION_RATE_MULTIPLIER: f64 = 0.01;
pub(super) const MAX_SIMULATION_RATE_MULTIPLIER: f64 = 100_000.0;
pub(super) const SIDE_PANEL_WIDTH_PX: f32 = 300.0;
pub(super) const STARFIELD_COUNT: usize = 900;
pub(super) const STARFIELD_RADIUS: f32 = 30_000.0;

// Free-camera tuning. Movement speed auto-scales with the distance to the
// nearest body so the same controls work whether you're creeping up to a
// surface or crossing interplanetary gaps.
pub(super) const FREE_CAMERA_SPEED_FACTOR: f32 = 0.6; // fraction of nearest-body distance per second
pub(super) const FREE_CAMERA_MIN_SPEED: f32 = 0.08; // scene units/s, so close-up never stalls
pub(super) const FREE_CAMERA_MAX_SPEED: f32 = 6_000.0; // scene units/s ceiling on big hops
pub(super) const FREE_CAMERA_BOOST_MULTIPLIER: f32 = 5.0; // hold Shift to boost
pub(super) const FREE_CAMERA_LOOK_SENSITIVITY: f32 = 0.0026; // radians per pixel of mouse motion

// Keyboard orbit-camera tuning — a pointer-free path to the mouse drag/scroll
// controls (accessibility: full keyboard parity). Held keys rotate and zoom
// continuously, scaled by frame time.
pub(super) const ORBIT_KEY_ROTATE_SPEED: f32 = 1.2; // radians/s of yaw or pitch while a key is held
pub(super) const ORBIT_KEY_ZOOM_RATE: f32 = 1.5; // exponential zoom rate per second while Q/E is held

#[derive(Clone, Copy)]
pub(super) struct RingSpec {
    pub(super) inner_radius: f32,
    pub(super) outer_radius: f32,
    pub(super) axial_tilt_degrees: f32,
}

#[derive(Clone, Copy)]
pub(super) struct BodySpec {
    pub(super) display_name: &'static str,
    pub(super) spice_target: &'static str,
    pub(super) visual_radius: f32,
    pub(super) color: [f32; 4],
    pub(super) texture_file: &'static str,
    // Optional translucent cloud map rendered as a thin shell just above the
    // surface (e.g. Venus). `None` for bodies with no separate cloud layer.
    pub(super) cloud_texture: Option<&'static str>,
    // Signed sidereal spin rate in radians per simulated second.
    // Positive = prograde, negative = retrograde.
    pub(super) spin_radians_per_second: f32,
    pub(super) mesh_subdivisions: u32,
    pub(super) metallic: f32,
    pub(super) roughness: f32,
    pub(super) emissive: [f32; 3],
    pub(super) atmosphere_scale: f32,
    pub(super) atmosphere_emissive: [f32; 4],
    // Physical info shown in the body details panel (not used for physics/rendering).
    pub(super) physical_radius_km: f64,
    pub(super) mass_kg: f64,
    // Orbital period and semi-major axis around the Sun. `None` for Sun itself
    // and for satellites (Moon, Charon), which orbit a primary rather than the Sun.
    pub(super) orbital_period_days: Option<f64>,
    pub(super) semi_major_axis_au: Option<f64>,
    pub(super) rings: Option<RingSpec>,
    // Spin pole direction in scene space (Y-up). Default ECLIPTIC_POLE_SCENE.
    // Used for the body's mesh orientation and as the orbit normal for any
    // satellite parented to it (e.g. Charon orbits in Pluto's equatorial plane).
    pub(super) pole_direction: [f32; 3],
}

// Ecliptic north pole expressed in scene space (Bevy Y-up). Default for every body
// whose obliquity isn't separately specified.
pub(super) const ECLIPTIC_POLE_SCENE: [f32; 3] = [0.0, 1.0, 0.0];

// Pluto's IAU 2009 spin pole (RA = 132.993°, Dec = -6.163°), converted from
// equatorial J2000 to ecliptic and remapped into scene space. Charon shares this
// pole — the system is mutually tidally locked.
pub(super) const PLUTO_POLE_SCENE: [f32; 3] = [-0.677_73, -0.387_86, -0.624_69];

const fn sidereal_spin_radians_per_second(sidereal_period_days: f64) -> f32 {
    (std::f64::consts::TAU / (sidereal_period_days * SECONDS_PER_DAY)) as f32
}

// Cloud shell radius as a multiple of the body's visual radius — thin enough to
// hug the surface, below any atmosphere-glow halo (`atmosphere_scale`).
pub(super) const CLOUD_LAYER_SCALE: f32 = 1.02;

// Cloud super-rotation rate. Venus's upper atmosphere circles the planet in ~4
// days (retrograde), vastly faster than its 243-day surface spin, so the cloud
// shell drifts visibly relative to the surface map beneath it.
pub(super) const CLOUD_SUPERROTATION_RADIANS_PER_SECOND: f32 =
    sidereal_spin_radians_per_second(-4.2);

#[derive(Resource)]
pub(super) struct AppStatus {
    pub(super) spice_enabled: bool,
    pub(super) status_line: String,
}

pub(super) struct HorizonsTargetSample {
    pub(super) index: usize,
    pub(super) display_name: &'static str,
    pub(super) command: &'static str,
    pub(super) spice_au: [f64; 3],
}

pub(super) struct HorizonsSyncTaskInput {
    pub(super) utc_timestamp: String,
    pub(super) target_count: usize,
    pub(super) targets: Vec<HorizonsTargetSample>,
    pub(super) initial_failures: Vec<String>,
}

pub(super) struct HorizonsSyncResult {
    pub(super) enabled: bool,
    pub(super) status_line: String,
    pub(super) failures: Vec<String>,
    pub(super) per_body_au_offset: Vec<DVec3>,
}

#[derive(Resource)]
pub(super) struct HorizonsSyncState {
    pub(super) enabled: bool,
    pub(super) status_line: String,
    pub(super) failures: Vec<String>,
    pub(super) per_body_au_offset: Vec<DVec3>,
    pub(super) task: Option<Task<HorizonsSyncResult>>,
    pub(super) retry_requested: bool,
    pub(super) retry_attempt: u32,
    pub(super) next_retry_deadline_seconds: Option<f64>,
}

impl HorizonsSyncState {
    pub(super) fn new(body_count: usize) -> Self {
        Self {
            enabled: false,
            status_line: "Horizons sync idle".to_string(),
            failures: Vec::new(),
            per_body_au_offset: vec![DVec3::ZERO; body_count],
            task: None,
            retry_requested: false,
            retry_attempt: 0,
            next_retry_deadline_seconds: None,
        }
    }
}

#[derive(Resource, Clone)]
pub(super) struct HorizonsHttpClient {
    pub(super) client: Client,
}

#[derive(Resource, Default)]
pub(super) struct PlanetTextureRegistry {
    pub(super) entries: Vec<PlanetTextureEntry>,
}

pub(super) struct PlanetTextureEntry {
    pub(super) body_name: &'static str,
    pub(super) path: String,
    pub(super) handle: Handle<Image>,
}

#[derive(Resource, Default)]
pub(super) struct TextureStatus {
    pub(super) summary: String,
    pub(super) failed: Vec<String>,
}

#[derive(Resource)]
pub(super) struct AppPaths {
    pub(super) assets_root: PathBuf,
}

#[derive(Resource)]
pub(super) struct EphemerisResource {
    pub(super) ephemeris: SpiceEphemeris,
}

#[derive(Resource)]
pub(super) struct SimulationState {
    pub(super) elapsed_simulation_days: f64,
    pub(super) simulation_rate: f64,
    pub(super) paused: bool,
    pub(super) selected_body_index: Option<usize>,
    pub(super) jump_request: Option<usize>,
    pub(super) target_filter: String,
    pub(super) picker_year: i32,
    pub(super) picker_month: u32,
    pub(super) picker_day: u32,
}

impl Default for SimulationState {
    fn default() -> Self {
        Self {
            elapsed_simulation_days: 0.0,
            simulation_rate: DEFAULT_SIMULATION_RATE_MULTIPLIER,
            paused: true,
            selected_body_index: None,
            jump_request: None,
            target_filter: String::new(),
            picker_year: 2025,
            picker_month: 1,
            picker_day: 1,
        }
    }
}

#[derive(Resource)]
pub(super) struct RenderSettings {
    pub(super) stars_enabled: bool,
    pub(super) atmosphere_enabled: bool,
    pub(super) trails_enabled: bool,
    pub(super) rings_enabled: bool,
    pub(super) orbits_enabled: bool,
    pub(super) asteroids_enabled: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            stars_enabled: true,
            atmosphere_enabled: false,
            trails_enabled: true,
            rings_enabled: true,
            orbits_enabled: false,
            asteroids_enabled: true,
        }
    }
}

#[derive(Resource)]
pub(super) struct SimulationEpoch {
    pub(super) start_utc: DateTime<Utc>,
}

#[derive(Resource)]
pub(super) struct BodyTrails {
    pub(super) points: Vec<VecDeque<Vec3>>,
}

impl BodyTrails {
    pub(super) fn new(body_count: usize) -> Self {
        Self {
            points: (0..body_count).map(|_| VecDeque::new()).collect(),
        }
    }

    pub(super) fn clear(&mut self) {
        for trail in &mut self.points {
            trail.clear();
        }
    }
}

#[derive(Resource)]
pub(super) struct BodyRuntime {
    pub(super) positions: Vec<DVec3>,
}

#[derive(Resource)]
pub(super) struct LightingRig {
    pub(super) solar_key: Entity,
    pub(super) sky_fill: Entity,
    pub(super) rim_fill: Entity,
}

/// Which camera scheme is currently driving the view. Orbit is the default
/// inspection camera (tethered to a target); Free is an untethered fly-cam.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum CameraMode {
    #[default]
    Orbit,
    Free,
}

#[derive(Resource)]
pub(super) struct OrbitCameraState {
    pub(super) mode: CameraMode,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    pub(super) distance: f32,
    pub(super) min_distance: f32,
    pub(super) max_distance: f32,
    pub(super) target: Vec3,
    pub(super) flight: Option<CameraFlight>,
    // Free-camera state: world-space position and look angles. Seeded from the
    // orbit camera on entering Free mode so the handoff is seamless.
    pub(super) free_position: Vec3,
    pub(super) free_yaw: f32,
    pub(super) free_pitch: f32,
}

#[derive(Clone, Copy)]
pub(super) struct CameraFlight {
    pub(super) target_index: usize,
    pub(super) target_distance: f32,
}

#[derive(Component)]
pub(super) struct MainCamera;

#[derive(Component)]
pub(super) struct BodyEntity {
    pub(super) index: usize,
}

#[derive(Component)]
pub(super) struct AtmosphereLayer;

#[derive(Component)]
pub(super) struct AtmosphereOf {
    pub(super) index: usize,
}

/// Marks the translucent cloud shell rendered just above a body's surface
/// (currently only Venus). Linked to its parent body by `CloudOf`.
#[derive(Component)]
pub(super) struct CloudLayer;

#[derive(Component)]
pub(super) struct CloudOf {
    pub(super) index: usize,
}

#[derive(Component)]
pub(super) struct PlanetRing;

#[derive(Component)]
pub(super) struct RingOf {
    pub(super) index: usize,
}

#[derive(Component)]
pub(super) struct StarsBackdrop;

#[derive(Clone, Copy)]
pub(super) struct StarPoint {
    pub(super) position: Vec3,
    pub(super) color: [f32; 4],
    pub(super) size: f32,
}

pub(super) const BODIES: [BodySpec; 18] = [
    BodySpec {
        display_name: "Sun",
        spice_target: "SUN",
        visual_radius: 3.8,
        color: [1.0, 0.9, 0.55, 1.0],
        texture_file: "sun.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(25.38),
        mesh_subdivisions: 96,
        metallic: 0.0,
        roughness: 0.6,
        emissive: [2.4, 1.7, 0.9],
        atmosphere_scale: 1.12,
        atmosphere_emissive: [0.95, 0.66, 0.24, 0.12],
        physical_radius_km: 695_700.0,
        mass_kg: 1.989e30,
        orbital_period_days: None,
        semi_major_axis_au: None,
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Mercury",
        spice_target: "MERCURY BARYCENTER",
        visual_radius: 0.06,
        color: [0.65, 0.62, 0.59, 1.0],
        texture_file: "mercury.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(58.646),
        mesh_subdivisions: 56,
        metallic: 0.03,
        roughness: 0.86,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 2_439.7,
        mass_kg: 3.3011e23,
        orbital_period_days: Some(87.969),
        semi_major_axis_au: Some(0.387),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Venus",
        spice_target: "VENUS BARYCENTER",
        visual_radius: 0.15,
        color: [0.92, 0.76, 0.4, 1.0],
        texture_file: "venus.jpg",
        cloud_texture: Some("venus_clouds.jpg"),
        spin_radians_per_second: sidereal_spin_radians_per_second(-243.025),
        mesh_subdivisions: 60,
        metallic: 0.02,
        roughness: 0.74,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 1.05,
        atmosphere_emissive: [0.8, 0.72, 0.52, 0.08],
        physical_radius_km: 6_051.8,
        mass_kg: 4.8675e24,
        orbital_period_days: Some(224.701),
        semi_major_axis_au: Some(0.723),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Earth",
        spice_target: "EARTH",
        // 15× physical size at 250 AU/unit; Moon orbit (0.642 scene units) leaves
        // a clear 0.44-unit gap between Earth and Moon surfaces.
        visual_radius: 0.16,
        color: [0.3, 0.5, 1.0, 1.0],
        texture_file: "earth.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(0.997_269_68),
        mesh_subdivisions: 64,
        metallic: 0.05,
        roughness: 0.52,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 1.03,
        atmosphere_emissive: [0.32, 0.58, 1.0, 0.09],
        physical_radius_km: 6_371.0,
        mass_kg: 5.972e24,
        orbital_period_days: Some(365.256),
        semi_major_axis_au: Some(1.0),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Moon",
        spice_target: "MOON",
        // 15× physical size at 250 AU/unit; proportional to Earth (Earth/Moon ≈ 3.67).
        visual_radius: 0.044,
        color: [0.84, 0.84, 0.8, 1.0],
        texture_file: "moon.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(27.321_661),
        mesh_subdivisions: 48,
        metallic: 0.01,
        roughness: 0.89,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 1_737.4,
        mass_kg: 7.342e22,
        orbital_period_days: None,
        semi_major_axis_au: None,
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Mars",
        spice_target: "MARS BARYCENTER",
        visual_radius: 0.085,
        color: [0.8, 0.35, 0.2, 1.0],
        texture_file: "mars.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(1.025_957),
        mesh_subdivisions: 56,
        metallic: 0.02,
        roughness: 0.7,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 1.02,
        atmosphere_emissive: [0.8, 0.35, 0.18, 0.06],
        physical_radius_km: 3_389.5,
        mass_kg: 6.4171e23,
        orbital_period_days: Some(686.98),
        semi_major_axis_au: Some(1.524),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Ceres",
        spice_target: "CERES",
        visual_radius: 0.04,
        color: [0.74, 0.74, 0.72, 1.0],
        texture_file: "ceres.png",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(0.3781),
        mesh_subdivisions: 40,
        metallic: 0.01,
        roughness: 0.9,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 469.7,
        mass_kg: 9.393e20,
        orbital_period_days: Some(1680.0),
        semi_major_axis_au: Some(2.767),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Vesta",
        spice_target: "VESTA",
        visual_radius: 0.03,
        color: [0.7, 0.66, 0.62, 1.0],
        texture_file: "vesta.png",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(0.2226),
        mesh_subdivisions: 40,
        metallic: 0.01,
        roughness: 0.9,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 262.7,
        mass_kg: 2.59076e20,
        orbital_period_days: Some(1325.0),
        semi_major_axis_au: Some(2.361),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Jupiter",
        spice_target: "JUPITER BARYCENTER",
        // Reduced from the default 15× to 4.6× so Galilean moons (innermost Io at
        // 0.704 scene units) orbit visibly outside the sphere.
        visual_radius: 0.55,
        color: [0.82, 0.66, 0.42, 1.0],
        texture_file: "jupiter.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(0.41354),
        mesh_subdivisions: 72,
        metallic: 0.0,
        roughness: 0.6,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 1.01,
        atmosphere_emissive: [0.8, 0.68, 0.42, 0.04],
        physical_radius_km: 69_911.0,
        mass_kg: 1.898e27,
        orbital_period_days: Some(4332.589),
        semi_major_axis_au: Some(5.204),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Io",
        spice_target: "IO",
        // 15× physical radius at 250 AU/unit; orbits Jupiter at 0.704 scene units.
        visual_radius: 0.046,
        color: [0.88, 0.65, 0.28, 1.0],
        texture_file: "io.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(1.769138),
        mesh_subdivisions: 48,
        metallic: 0.01,
        roughness: 0.85,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 1_821.6,
        mass_kg: 8.932e22,
        orbital_period_days: None,
        semi_major_axis_au: None,
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Europa",
        spice_target: "EUROPA",
        // 15× physical radius at 250 AU/unit; orbits Jupiter at 1.12 scene units.
        visual_radius: 0.039,
        color: [0.94, 0.91, 0.84, 1.0],
        texture_file: "europa.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(3.551181),
        mesh_subdivisions: 44,
        metallic: 0.02,
        roughness: 0.55,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 1_560.8,
        mass_kg: 4.7998e22,
        orbital_period_days: None,
        semi_major_axis_au: None,
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Ganymede",
        spice_target: "GANYMEDE",
        // 15× physical radius at 250 AU/unit; orbits Jupiter at 1.79 scene units.
        visual_radius: 0.066,
        color: [0.68, 0.65, 0.60, 1.0],
        texture_file: "ganymede.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(7.154553),
        mesh_subdivisions: 52,
        metallic: 0.01,
        roughness: 0.82,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 2_634.1,
        mass_kg: 1.4819e23,
        orbital_period_days: None,
        semi_major_axis_au: None,
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Callisto",
        spice_target: "CALLISTO",
        // 15× physical radius at 250 AU/unit; orbits Jupiter at 3.15 scene units.
        visual_radius: 0.060,
        color: [0.46, 0.44, 0.42, 1.0],
        texture_file: "callisto.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(16.689018),
        mesh_subdivisions: 48,
        metallic: 0.01,
        roughness: 0.9,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 2_410.3,
        mass_kg: 1.0759e23,
        orbital_period_days: None,
        semi_major_axis_au: None,
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Saturn",
        spice_target: "SATURN BARYCENTER",
        visual_radius: 1.5,
        color: [0.83, 0.77, 0.56, 1.0],
        texture_file: "saturn.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(0.444),
        mesh_subdivisions: 72,
        metallic: 0.0,
        roughness: 0.62,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 1.01,
        atmosphere_emissive: [0.82, 0.74, 0.55, 0.04],
        physical_radius_km: 58_232.0,
        mass_kg: 5.683e26,
        orbital_period_days: Some(10_759.22),
        semi_major_axis_au: Some(9.582),
        rings: Some(RingSpec {
            inner_radius: 1.72,
            outer_radius: 3.53,
            axial_tilt_degrees: 26.73,
        }),
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Uranus",
        spice_target: "URANUS BARYCENTER",
        visual_radius: 0.64,
        color: [0.57, 0.82, 0.92, 1.0],
        texture_file: "uranus.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(-0.71833),
        mesh_subdivisions: 64,
        metallic: 0.0,
        roughness: 0.45,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 1.02,
        atmosphere_emissive: [0.52, 0.82, 0.92, 0.05],
        physical_radius_km: 25_362.0,
        mass_kg: 8.681e25,
        orbital_period_days: Some(30_688.5),
        semi_major_axis_au: Some(19.201),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Neptune",
        spice_target: "NEPTUNE BARYCENTER",
        visual_radius: 0.62,
        color: [0.35, 0.45, 0.95, 1.0],
        texture_file: "neptune.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(0.67125),
        mesh_subdivisions: 64,
        metallic: 0.0,
        roughness: 0.5,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 1.02,
        atmosphere_emissive: [0.32, 0.46, 0.96, 0.06],
        physical_radius_km: 24_622.0,
        mass_kg: 1.024e26,
        orbital_period_days: Some(60_182.0),
        semi_major_axis_au: Some(30.047),
        rings: None,
        pole_direction: ECLIPTIC_POLE_SCENE,
    },
    BodySpec {
        display_name: "Pluto",
        spice_target: "PLUTO BARYCENTER",
        // Capped below 15× physical to keep Charon visibly separate
        // (Pluto–Charon orbit is only 0.033 scene units at 250 AU/unit).
        visual_radius: 0.018,
        color: [0.82, 0.76, 0.68, 1.0],
        texture_file: "pluto.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(-6.38723),
        mesh_subdivisions: 48,
        metallic: 0.0,
        roughness: 0.86,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 1_188.3,
        mass_kg: 1.303e22,
        orbital_period_days: Some(90_560.0),
        semi_major_axis_au: Some(39.482),
        rings: None,
        pole_direction: PLUTO_POLE_SCENE,
    },
    BodySpec {
        display_name: "Charon",
        spice_target: "CHARON",
        // Capped below 15× physical to keep separation from Pluto.
        visual_radius: 0.009,
        color: [0.74, 0.74, 0.72, 1.0],
        texture_file: "charon.jpg",
        cloud_texture: None,
        spin_radians_per_second: sidereal_spin_radians_per_second(-6.38723),
        mesh_subdivisions: 36,
        metallic: 0.0,
        roughness: 0.9,
        emissive: [0.0, 0.0, 0.0],
        atmosphere_scale: 0.0,
        atmosphere_emissive: [0.0, 0.0, 0.0, 0.0],
        physical_radius_km: 606.0,
        mass_kg: 1.586e21,
        orbital_period_days: None,
        semi_major_axis_au: None,
        rings: None,
        pole_direction: PLUTO_POLE_SCENE,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn au_to_scene_units_is_realistic_scale() {
        assert_eq!(AU_TO_SCENE_UNITS, 250.0);
    }

    #[test]
    fn sidereal_spin_radians_per_second_preserves_retrograde_sign() {
        assert!(sidereal_spin_radians_per_second(1.0) > 0.0);
        assert!(sidereal_spin_radians_per_second(-1.0) < 0.0);
    }
}
