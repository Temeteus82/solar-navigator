mod asteroids;
mod camera;
mod materials;
#[cfg(feature = "raytracing")]
mod raytracing;
mod render;
mod setup;
mod simulation;
mod theme;
mod types;
mod ui;
mod util;

use crate::ephemeris::{SpiceEphemeris, build_horizons_client};
use bevy::log::{BoxedLayer, LogPlugin, tracing_subscriber};
use bevy::math::DVec3;
use bevy::pbr::MaterialPlugin;
use bevy::post_process::auto_exposure::AutoExposurePlugin;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};
use chrono::Datelike;
use materials::{PlanetAtmosphereMaterial, PlanetRingMaterial};
use std::f32::consts::PI;
use std::sync::Mutex;
use std::time::Duration;
use types::{
    AppPaths, AppStatus, BODIES, BodyRuntime, BodyTrails, DatePickerRange, EphemerisResource,
    HorizonsHttpClient, HorizonsSyncState, OrbitCameraState, RenderOrigin, RenderSettings,
    SimulationEpoch, SimulationState, TextureStatus,
};

/// Log file mirroring the terminal output, written to the working directory and
/// overwritten on every run.
const LOG_FILE_NAME: &str = "solar-navigator.log";

/// `LogPlugin::custom_layer`: a second formatter that writes every event the
/// terminal shows (same filter, no ANSI colours) to `LOG_FILE_NAME`. Panics and
/// anything printed straight to stderr bypass it.
fn log_file_layer(_app: &mut App) -> Option<BoxedLayer> {
    let file = match std::fs::File::create(LOG_FILE_NAME) {
        Ok(file) => file,
        Err(err) => {
            // The logger is what's being built, so stderr is all there is.
            eprintln!("Could not create {LOG_FILE_NAME}: {err}");
            return None;
        }
    };
    Some(Box::new(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(Mutex::new(file)),
    ))
}

pub(crate) fn run() {
    let assets_root = util::resolve_assets_root();
    let asset_file_path = assets_root.to_string_lossy().to_string();
    let spice_dir = assets_root.join("spice");

    let ephemeris = SpiceEphemeris::new(&spice_dir);
    let status_line = ephemeris.status_line().to_string();
    let spice_enabled = ephemeris.is_spice_enabled();
    let date_picker_range = DatePickerRange::from_coverage(ephemeris.spice_coverage_unix_days());
    // The one launch instant: the ephemeris anchors every analytic position
    // and the SPICE epoch to it, and the panel's date label and picker start
    // from the same value, so simulation day 0 means one date everywhere.
    let start_utc = ephemeris.start_utc();

    let horizons_client = build_horizons_client(Duration::from_secs(2));
    let horizons_client_error = horizons_client.as_ref().err().cloned();

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgba(0.003, 0.005, 0.02, 1.0)))
        .insert_resource(AppPaths { assets_root })
        .insert_resource(AppStatus {
            spice_enabled,
            status_line,
        })
        .insert_resource(HorizonsSyncState::new(BODIES.len()))
        .insert_resource(TextureStatus::default())
        .insert_resource(SimulationState {
            picker_year: start_utc.year(),
            picker_month: start_utc.month(),
            picker_day: start_utc.day(),
            ..SimulationState::default()
        })
        .insert_resource(RenderSettings::default())
        .insert_resource(BodyRuntime {
            positions: vec![DVec3::ZERO; BODIES.len()],
        })
        .insert_resource(BodyTrails::new(BODIES.len()))
        .insert_resource(RenderOrigin::default())
        .insert_resource(SimulationEpoch { start_utc })
        .insert_resource(date_picker_range)
        .insert_resource(OrbitCameraState {
            mode: types::CameraMode::default(),
            yaw: PI,
            pitch: (55.0_f32 / 188.3_f32).asin(),
            distance: 188.3,
            min_distance: 0.05,
            max_distance: 30_000.0,
            target: DVec3::ZERO,
            pan_offset: DVec3::ZERO,
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
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_file_path,
                    ..default()
                })
                .set(LogPlugin {
                    custom_layer: log_file_layer,
                    ..default()
                }),
        )
        .add_plugins(MaterialPlugin::<PlanetAtmosphereMaterial>::default())
        .add_plugins(MaterialPlugin::<PlanetRingMaterial>::default())
        .add_plugins(AutoExposurePlugin)
        // Bindless egui page-faulted the GPU on DX12 (RGD capture, 2026-10-16:
        // the sampler-array lookup in egui.wgsl's fs_main). The panel draws only a
        // couple of textures, so bindless saves next to nothing anyway.
        .add_plugins(EguiPlugin {
            bindless_mode_array_size: None,
        })
        // These are known before the App exists, but logging only starts with
        // `LogPlugin`, so report them from the first frame to reach the log file.
        .add_systems(Startup, move |app_status: Res<AppStatus>| {
            info!("{}", app_status.status_line);
            if let Some(err) = &horizons_client_error {
                warn!("Horizons HTTP client unavailable: {err}");
            }
        })
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
                // Input that changes the clock runs before this frame's
                // ephemeris pass, so a pause or Backspace reset shows at once.
                (
                    simulation::keyboard_controls,
                    simulation::advance_simulation_time,
                    simulation::update_body_positions,
                )
                    .chain(),
                // Everything that reads `BodyRuntime::positions` waits for
                // the ephemeris pass. `update_body_positions` is a NonSend
                // system pinned to the main thread; without these edges a
                // reader could run first on a worker and place an atmosphere
                // or ring where its body was last frame — at the 100 000×
                // speed cap that is half an Earth-halo radius behind.
                (
                    simulation::sync_atmosphere_positions,
                    simulation::sync_cloud_layers,
                    simulation::sync_ring_positions,
                    simulation::sync_ring_material_uniforms,
                    camera::toggle_camera_mode,
                    camera::free_camera_input,
                    // A jump sets the flight and clears any pan; tracking
                    // yields to a flight, and the flight itself is last.
                    (
                        simulation::apply_body_scale,
                        camera::orbit_camera_input,
                        camera::handle_jump_requests,
                        camera::track_selected_body,
                        camera::apply_camera_flight,
                    )
                        .chain(),
                )
                    .after(simulation::update_body_positions),
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
                render::record_body_trails.after(simulation::update_body_positions),
                render::draw_body_trails.after(camera::update_camera_transform),
                render::draw_orbit_paths
                    .after(camera::update_camera_transform)
                    .after(simulation::update_body_positions),
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
        );

    // The raytracing experiment measures GPU cost by frame time, which the
    // 60 fps cap would hide; `raytracing.rs` also turns vsync off for this.
    #[cfg(feature = "raytracing")]
    let capped = !raytracing::uncapped();
    #[cfg(not(feature = "raytracing"))]
    let capped = true;
    if capped {
        app.add_systems(Last, render::cap_frame_rate);
    }

    #[cfg(feature = "raytracing")]
    app.add_plugins(raytracing::RaytracingPlugin);

    if let Ok(client) = horizons_client {
        app.insert_resource(HorizonsHttpClient { client });
    }

    app.run();
}
