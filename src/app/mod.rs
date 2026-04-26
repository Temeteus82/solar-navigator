mod camera;
mod materials;
mod render;
mod setup;
mod simulation;
mod types;
mod ui;
mod util;

use crate::ephemeris::{SpiceEphemeris, build_horizons_client};
use bevy::light::PointLightShadowMap;
use bevy::math::DVec3;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use chrono::{Datelike, Utc};
use materials::PlanetAtmosphereMaterial;
use std::f32::consts::PI;
use std::time::Duration;
use types::{
    AppPaths, AppStatus, BODIES, BodyRuntime, BodyTrails, EphemerisResource, HorizonsHttpClient,
    HorizonsSyncState, OrbitCameraState, RenderSettings, SimulationEpoch, SimulationState,
    TextureStatus,
};

pub(crate) fn run() {
    let assets_root = util::resolve_assets_root();
    let asset_file_path = assets_root.to_string_lossy().to_string();
    let spice_dir = assets_root.join("spice");

    let ephemeris = SpiceEphemeris::new(&spice_dir);
    let status_line = ephemeris.status_line().to_string();
    let spice_enabled = ephemeris.is_spice_enabled();
    eprintln!("{status_line}");

    let horizons_client = match build_horizons_client(Duration::from_secs(2)) {
        Ok(client) => Some(client),
        Err(err) => {
            eprintln!("Horizons HTTP client unavailable: {err}");
            None
        }
    };

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgba(0.003, 0.005, 0.02, 1.0)))
        .insert_resource(PointLightShadowMap { size: 2048 })
        .insert_resource(AppPaths { assets_root })
        .insert_resource(AppStatus {
            spice_enabled,
            status_line,
        })
        .insert_resource(HorizonsSyncState::new(BODIES.len()))
        .insert_resource(TextureStatus::default())
        .insert_resource({
            let now = Utc::now();
            SimulationState {
                picker_year: now.year(),
                picker_month: now.month(),
                picker_day: now.day(),
                ..SimulationState::default()
            }
        })
        .insert_resource(RenderSettings::default())
        .insert_resource(BodyRuntime {
            positions: vec![DVec3::ZERO; BODIES.len()],
        })
        .insert_resource(BodyTrails::new(BODIES.len()))
        .insert_resource(SimulationEpoch {
            start_utc: Utc::now(),
        })
        .insert_resource(OrbitCameraState {
            yaw: PI,
            pitch: (55.0_f32 / 188.3_f32).asin(),
            distance: 188.3,
            min_distance: 3.0,
            max_distance: 30_000.0,
            target: Vec3::ZERO,
            flight: None,
        })
        .insert_non_send_resource(EphemerisResource { ephemeris })
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            file_path: asset_file_path,
            ..default()
        }))
        .add_plugins(MaterialPlugin::<PlanetAtmosphereMaterial>::default())
        .add_plugins(EguiPlugin::default())
        .add_systems(
            Startup,
            (setup::setup_scene, setup::start_horizons_sync).chain(),
        )
        .add_systems(
            Update,
            (
                simulation::keyboard_controls,
                simulation::advance_simulation_time,
                camera::handle_jump_requests,
                camera::orbit_camera_input,
                simulation::update_body_positions,
                camera::track_selected_body,
                simulation::sync_atmosphere_positions,
                simulation::sync_ring_positions,
                camera::apply_camera_flight,
                setup::process_horizons_sync_requests,
                setup::poll_horizons_sync_task,
                setup::refresh_texture_status,
                setup::sync_environment_lighting_from_sky,
            ),
        )
        .add_systems(
            Update,
            (
                camera::update_camera_transform,
                render::apply_lighting_preset,
                render::sync_visibility_toggles,
                render::record_body_trails,
                render::draw_body_trails,
                render::draw_orbit_paths,
                render::update_window_title,
            ),
        )
        .add_systems(EguiPrimaryContextPass, ui::draw_side_panel);

    if let Some(client) = horizons_client {
        app.insert_resource(HorizonsHttpClient { client });
    }

    app.run();
}
