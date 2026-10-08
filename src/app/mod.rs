mod asteroids;
mod camera;
mod materials;
mod render;
mod setup;
mod simulation;
mod theme;
mod types;
mod ui;
mod util;

use crate::ephemeris::{SpiceEphemeris, build_horizons_client};
use bevy::light::PointLightShadowMap;
use bevy::math::DVec3;
use bevy::pbr::MaterialPlugin;
use bevy::post_process::auto_exposure::AutoExposurePlugin;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};
use chrono::{Datelike, Utc};
use materials::{PlanetAtmosphereMaterial, PlanetRingMaterial};
use std::f32::consts::PI;
use std::time::Duration;
use types::{
    AppPaths, AppStatus, BODIES, BodyRuntime, BodyTrails, EphemerisResource, HorizonsHttpClient,
    HorizonsSyncState, OrbitCameraState, RenderOrigin, RenderSettings, SimulationEpoch,
    SimulationState, TextureStatus,
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

    let default_plugins = DefaultPlugins.set(AssetPlugin {
        file_path: asset_file_path,
        ..default()
    });
    // Windows renders through DX12 only. wgpu otherwise picks Vulkan on AMD,
    // and the app froze the whole system (hard hang, no driver recovery logged)
    // while a browser was decoding video on the same GPU. `WGPU_BACKEND` still
    // overrides this, e.g. to capture Vulkan in docs/gpu-profiling.md.
    #[cfg(windows)]
    let default_plugins = default_plugins.set(bevy::render::RenderPlugin {
        render_creation: bevy::render::settings::WgpuSettings {
            backends: Some(
                bevy::render::settings::Backends::from_env()
                    .unwrap_or(bevy::render::settings::Backends::DX12),
            ),
            ..default()
        }
        .into(),
        ..default()
    });

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
        .insert_resource(RenderOrigin::default())
        .insert_resource(SimulationEpoch {
            start_utc: Utc::now(),
        })
        .insert_resource(OrbitCameraState {
            mode: types::CameraMode::default(),
            yaw: PI,
            pitch: (55.0_f32 / 188.3_f32).asin(),
            distance: 188.3,
            min_distance: 0.05,
            max_distance: 30_000.0,
            target: DVec3::ZERO,
            flight: None,
            free_position: DVec3::ZERO,
            free_yaw: 0.0,
            free_pitch: 0.0,
        })
        .insert_non_send(EphemerisResource { ephemeris })
        // We spawn our own dedicated UI-only camera for egui in setup::setup_scene
        // (see the comment there) instead of letting bevy_egui auto-attach a
        // primary context to the first camera it finds.
        .insert_resource(EguiGlobalSettings {
            auto_create_primary_context: false,
            ..default()
        })
        .add_plugins(default_plugins)
        .add_plugins(MaterialPlugin::<PlanetAtmosphereMaterial>::default())
        .add_plugins(MaterialPlugin::<PlanetRingMaterial>::default())
        .add_plugins(AutoExposurePlugin)
        .add_plugins(EguiPlugin::default())
        .add_systems(
            Startup,
            (
                setup::setup_scene,
                setup::start_horizons_sync,
                asteroids::spawn_asteroid_belt,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                simulation::keyboard_controls,
                simulation::advance_simulation_time,
                camera::toggle_camera_mode,
                camera::handle_jump_requests,
                camera::orbit_camera_input,
                camera::free_camera_input,
                simulation::update_body_positions,
                camera::track_selected_body,
                simulation::sync_atmosphere_positions,
                simulation::sync_cloud_layers,
                simulation::sync_ring_positions,
                simulation::sync_ring_material_uniforms,
                camera::apply_camera_flight,
                setup::process_horizons_sync_requests,
                setup::poll_horizons_sync_task,
                setup::refresh_texture_status,
                setup::sync_environment_lighting_from_sky,
                asteroids::update_asteroid_positions,
                asteroids::sync_asteroid_visibility,
                setup::set_window_icon,
            ),
        )
        .add_systems(
            Update,
            (
                // Fixes this frame's floating origin, so it runs after every
                // system that moves the camera; everything that reads
                // `RenderOrigin` in `Update` runs after it.
                camera::update_camera_transform
                    .after(camera::handle_jump_requests)
                    .after(camera::toggle_camera_mode)
                    .after(camera::orbit_camera_input)
                    .after(camera::free_camera_input)
                    .after(camera::track_selected_body)
                    .after(camera::apply_camera_flight),
                render::sync_shader_sun_positions.after(camera::update_camera_transform),
                render::apply_lighting_preset,
                render::scale_view_dependent_effects,
                render::sync_visibility_toggles,
                render::record_body_trails,
                render::draw_body_trails.after(camera::update_camera_transform),
                render::draw_orbit_paths.after(camera::update_camera_transform),
                render::update_window_title,
            ),
        )
        .add_systems(
            PostUpdate,
            render::apply_render_origin.before(TransformSystems::Propagate),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (theme::apply_theme, ui::draw_side_panel).chain(),
        )
        .add_systems(Last, render::cap_frame_rate);

    if let Some(client) = horizons_client {
        app.insert_resource(HorizonsHttpClient { client });
    }

    app.run();
}
