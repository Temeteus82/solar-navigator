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

#[derive(Clone, Copy)]
pub(super) struct BodySpec {
    pub(super) display_name: &'static str,
    pub(super) spice_target: &'static str,
    pub(super) visual_radius: f32,
    pub(super) color: [f32; 4],
    pub(super) texture_file: &'static str,
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
}

const fn sidereal_spin_radians_per_second(sidereal_period_days: f64) -> f32 {
    (std::f64::consts::TAU / (sidereal_period_days * SECONDS_PER_DAY)) as f32
}

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
}

impl Default for SimulationState {
    fn default() -> Self {
        Self {
            elapsed_simulation_days: 0.0,
            simulation_rate: DEFAULT_SIMULATION_RATE_MULTIPLIER,
            paused: false,
            selected_body_index: None,
            jump_request: None,
            target_filter: String::new(),
        }
    }
}

#[derive(Resource)]
pub(super) struct RenderSettings {
    pub(super) stars_enabled: bool,
    pub(super) atmosphere_enabled: bool,
    pub(super) trails_enabled: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            stars_enabled: true,
            atmosphere_enabled: false,
            trails_enabled: true,
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

#[derive(Resource)]
pub(super) struct OrbitCameraState {
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    pub(super) distance: f32,
    pub(super) min_distance: f32,
    pub(super) max_distance: f32,
    pub(super) target: Vec3,
    pub(super) flight: Option<CameraFlight>,
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

#[derive(Component)]
pub(super) struct StarsBackdrop;

#[derive(Clone, Copy)]
pub(super) struct StarPoint {
    pub(super) position: Vec3,
    pub(super) color: [f32; 4],
    pub(super) size: f32,
}

pub(super) const BODIES: [BodySpec; 14] = [
    BodySpec {
        display_name: "Sun",
        spice_target: "SUN",
        visual_radius: 3.8,
        color: [1.0, 0.9, 0.55, 1.0],
        texture_file: "sun.jpg",
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
    },
    BodySpec {
        display_name: "Mercury",
        spice_target: "MERCURY BARYCENTER",
        visual_radius: 0.06,
        color: [0.65, 0.62, 0.59, 1.0],
        texture_file: "mercury.jpg",
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
    },
    BodySpec {
        display_name: "Venus",
        spice_target: "VENUS BARYCENTER",
        visual_radius: 0.15,
        color: [0.92, 0.76, 0.4, 1.0],
        texture_file: "venus.jpg",
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
    },
    BodySpec {
        display_name: "Earth",
        spice_target: "EARTH",
        // 15× physical size at 250 AU/unit; Moon orbit (0.642 scene units) leaves
        // a clear 0.44-unit gap between Earth and Moon surfaces.
        visual_radius: 0.16,
        color: [0.3, 0.5, 1.0, 1.0],
        texture_file: "earth.jpg",
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
    },
    BodySpec {
        display_name: "Moon",
        spice_target: "MOON",
        // 15× physical size at 250 AU/unit; proportional to Earth (Earth/Moon ≈ 3.67).
        visual_radius: 0.044,
        color: [0.84, 0.84, 0.8, 1.0],
        texture_file: "moon.jpg",
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
    },
    BodySpec {
        display_name: "Mars",
        spice_target: "MARS BARYCENTER",
        visual_radius: 0.085,
        color: [0.8, 0.35, 0.2, 1.0],
        texture_file: "mars.jpg",
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
    },
    BodySpec {
        display_name: "Ceres",
        spice_target: "CERES",
        visual_radius: 0.04,
        color: [0.74, 0.74, 0.72, 1.0],
        texture_file: "ceres.jpg",
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
    },
    BodySpec {
        display_name: "Vesta",
        spice_target: "VESTA",
        visual_radius: 0.03,
        color: [0.7, 0.66, 0.62, 1.0],
        texture_file: "vesta.jpg",
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
    },
    BodySpec {
        display_name: "Jupiter",
        spice_target: "JUPITER BARYCENTER",
        visual_radius: 1.8,
        color: [0.82, 0.66, 0.42, 1.0],
        texture_file: "jupiter.jpg",
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
    },
    BodySpec {
        display_name: "Saturn",
        spice_target: "SATURN BARYCENTER",
        visual_radius: 1.5,
        color: [0.83, 0.77, 0.56, 1.0],
        texture_file: "saturn.jpg",
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
    },
    BodySpec {
        display_name: "Uranus",
        spice_target: "URANUS BARYCENTER",
        visual_radius: 0.64,
        color: [0.57, 0.82, 0.92, 1.0],
        texture_file: "uranus.jpg",
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
    },
    BodySpec {
        display_name: "Neptune",
        spice_target: "NEPTUNE BARYCENTER",
        visual_radius: 0.62,
        color: [0.35, 0.45, 0.95, 1.0],
        texture_file: "neptune.jpg",
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
    },
    BodySpec {
        display_name: "Pluto",
        spice_target: "PLUTO BARYCENTER",
        // Capped below 15× physical to keep Charon visibly separate
        // (Pluto–Charon orbit is only 0.033 scene units at 250 AU/unit).
        visual_radius: 0.018,
        color: [0.82, 0.76, 0.68, 1.0],
        texture_file: "pluto.jpg",
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
    },
    BodySpec {
        display_name: "Charon",
        spice_target: "CHARON",
        // Capped below 15× physical to keep separation from Pluto.
        visual_radius: 0.009,
        color: [0.74, 0.74, 0.72, 1.0],
        texture_file: "charon.jpg",
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
