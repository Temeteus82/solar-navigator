use super::materials::{PlanetAtmosphereMaterial, PlanetRingMaterial};
use super::types::{
    AppPaths, AppStatus, AtmosphereLayer, AtmosphereOf, BODIES, BodyEntity, EphemerisResource,
    HorizonsHttpClient, HorizonsSyncResult, HorizonsSyncState, HorizonsSyncTaskInput,
    HorizonsTargetSample, KM_PER_AU, LightingRig, MainCamera, PlanetRing, PlanetTextureEntry,
    PlanetTextureRegistry, RingOf, StarsBackdrop, TextureStatus,
};
use super::util::{
    color_from_rgba, eclipj2000_to_scene, equirectangular_to_cubemap_image, linear_from_rgb,
    ring_mesh, spawn_fallback_starfield, sphere_mesh, white_pixel_image,
};
use crate::ephemeris::{
    fetch_horizons_heliocentric_position_au_with_client, horizons_command_for_target,
};
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{GeneratedEnvironmentMapLight, NotShadowCaster};
use bevy::math::DVec3;
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::post_process::auto_exposure::AutoExposure;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, futures_lite::future};
use chrono::Utc;

static ICON_PNG_SMALL: &[u8] = include_bytes!("../../assets/icon/AppIcon.iconset/icon_32x32.png");
static ICON_PNG_LARGE: &[u8] = include_bytes!("../../assets/icon/AppIcon.iconset/icon_256x256.png");

const HORIZONS_RETRY_BASE_DELAY_SECS: f64 = 1.0;
const HORIZONS_RETRY_MAX_DELAY_SECS: f64 = 30.0;
const HORIZONS_RETRY_MAX_ATTEMPTS: u32 = 5;

#[derive(Resource)]
pub(super) struct SkyEnvironmentState {
    source_equirect_texture: Handle<Image>,
    generated_cubemap: Option<Handle<Image>>,
    applied_to_camera: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn setup_scene(
    mut commands: Commands,
    paths: Res<AppPaths>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut atmosphere_materials: ResMut<Assets<PlanetAtmosphereMaterial>>,
    mut ring_materials: ResMut<Assets<PlanetRingMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
) {
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 0.3,
        ..default()
    });

    let camera_translation = Vec3::new(0.0, 55.0, -180.0);
    commands.spawn((
        Camera3d::default(),
        Msaa::Off,
        Tonemapping::AcesFitted,
        // Auto-exposure adapts to each planet's local light level so outer
        // planets aren't crushed to black by the Sun's inverse-square falloff.
        // Range is widened past the default (±8 stops) because solar-system
        // luminance spans ~12 stops from Mercury to Neptune.
        AutoExposure {
            range: -2.0..=2.0,
            speed_brighten: 2.0,
            speed_darken: 3.0,
            ..AutoExposure::default()
        },
        Bloom {
            intensity: 0.11,
            high_pass_frequency: 0.92,
            ..Bloom::NATURAL
        },
        Transform::from_translation(camera_translation).looking_at(Vec3::ZERO, Vec3::Y),
        MainCamera,
        // Depth and normal prepasses feed SSAO.
        DepthPrepass,
        NormalPrepass,
        // SSAO adds subtle ambient occlusion at sphere edges and
        // the planet-space terminator boundary.
        ScreenSpaceAmbientOcclusion::default(),
    ));

    let solar_key = commands
        .spawn((
            PointLight {
                intensity: 1_600_000_000.0,
                range: 14_000.0,
                color: Color::srgb(1.0, 0.97, 0.9),
                // Cast shadows so bodies occlude each other (eclipse geometry
                // is visible when zoomed into the Earth–Moon or Pluto–Charon systems).
                shadows_enabled: true,
                ..default()
            },
            Transform::from_translation(Vec3::ZERO),
        ))
        .id();

    let sky_fill = commands
        .spawn((
            DirectionalLight {
                illuminance: 5.0,
                color: Color::srgb(0.3, 0.35, 0.45),
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(Vec3::new(0.0, 1.0, 0.0))
                .looking_to(Vec3::new(-0.2, -1.0, 0.25), Vec3::Y),
        ))
        .id();

    let rim_fill = commands
        .spawn((
            PointLight {
                intensity: 0.0,
                range: 2_700.0,
                color: Color::srgb(0.47, 0.55, 0.78),
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(Vec3::new(-150.0, 100.0, 220.0)),
        ))
        .id();

    commands.insert_resource(LightingRig {
        solar_key,
        sky_fill,
        rim_fill,
    });

    let texture_dir = paths.assets_root.join("textures");

    let mut texture_registry = PlanetTextureRegistry::default();

    for (index, spec) in BODIES.iter().enumerate() {
        let sphere_handle = sphere_mesh(&mut meshes, spec.visual_radius, spec.mesh_subdivisions);

        let texture_path = texture_dir.join(spec.texture_file);
        let texture_handle = texture_path.is_file().then(|| {
            let relative_path = format!("textures/{}", spec.texture_file);
            let handle = asset_server.load(relative_path.clone());
            texture_registry.entries.push(PlanetTextureEntry {
                body_name: spec.display_name,
                path: relative_path,
                handle: handle.clone(),
            });
            handle
        });

        if texture_handle.is_none() {
            eprintln!(
                "Texture file missing for {} at {}",
                spec.display_name,
                texture_path.display()
            );
        }

        let base_color = if texture_handle.is_some() {
            Color::WHITE
        } else {
            color_from_rgba(spec.color)
        };

        let material = materials.add(StandardMaterial {
            base_color,
            base_color_texture: texture_handle,
            metallic: spec.metallic,
            perceptual_roughness: spec.roughness,
            // Lower reflectance reduces broad specular wash on textured planets.
            reflectance: 0.08,
            emissive: linear_from_rgb(spec.emissive),
            cull_mode: None,
            ..default()
        });

        let body_entity = commands
            .spawn((
                Mesh3d(sphere_handle),
                MeshMaterial3d(material),
                // Bevy UV-sphere mesh has poles on +Z/-Z; rotate so the local +Z axis
                // aligns with the body's spin pole. Per-frame `rotate_local_z` then
                // spins the texture around that axis. Matches `Quat::from_rotation_x(-FRAC_PI_2)`
                // for the default ecliptic-Y pole and tilts e.g. Pluto onto its side.
                Transform::from_rotation(Quat::from_rotation_arc(
                    Vec3::Z,
                    Vec3::from_array(spec.pole_direction).normalize(),
                )),
                BodyEntity { index },
            ))
            .id();

        if index == 0 {
            commands.entity(body_entity).insert(NotShadowCaster);
        }

        if spec.atmosphere_scale > 1.0 {
            let atmosphere_mesh = sphere_mesh(
                &mut meshes,
                spec.visual_radius * spec.atmosphere_scale,
                spec.mesh_subdivisions,
            );

            let atmosphere_material = atmosphere_materials.add(PlanetAtmosphereMaterial {
                tint: color_from_rgba(spec.atmosphere_emissive).to_linear(),
                // density, rim power, forward phase power, brightness
                params: Vec4::new(
                    (spec.atmosphere_emissive[3] * 1.75).clamp(0.05, 0.35),
                    2.4,
                    8.0,
                    1.6,
                ),
            });

            commands.spawn((
                Mesh3d(atmosphere_mesh),
                MeshMaterial3d(atmosphere_material),
                Transform::default(),
                AtmosphereLayer,
                AtmosphereOf { index },
                NotShadowCaster,
            ));
        }

        if let Some(ring) = spec.rings {
            let ring_tex_path = texture_dir.join("saturn_ring.png");
            let ring_texture = if ring_tex_path.is_file() {
                asset_server.load::<Image>("textures/saturn_ring.png")
            } else {
                // PlanetRingMaterial requires a texture handle; fall back to a
                // 1x1 white pixel so the tint colour drives the appearance.
                images.add(white_pixel_image())
            };
            let ring_handle = ring_mesh(&mut meshes, ring.inner_radius, ring.outer_radius, 128);
            let ring_material = ring_materials.add(PlanetRingMaterial {
                tint: Color::srgba(0.83, 0.77, 0.56, 0.92).to_linear(),
                // x = inner_radius, y = outer_radius, z = planet_radius (umbra),
                // w = ring_brightness
                params: Vec4::new(
                    ring.inner_radius,
                    ring.outer_radius,
                    spec.visual_radius,
                    1.15,
                ),
                // x = forward_scatter, y = back_scatter, z = specular, w = ambient
                lighting: Vec4::new(0.65, 0.45, 0.35, 0.06),
                // Updated each frame from BodyRuntime::positions.
                planet_position: Vec4::ZERO,
                color_texture: ring_texture,
            });
            let tilt = Quat::from_rotation_x(ring.axial_tilt_degrees.to_radians());
            commands.spawn((
                Mesh3d(ring_handle),
                MeshMaterial3d(ring_material),
                Transform::from_rotation(tilt),
                PlanetRing,
                RingOf { index },
                NotShadowCaster,
            ));
        }
    }

    commands.insert_resource(texture_registry);

    let milky_way_path = texture_dir.join("milky_way_8k.jpg");
    let starfield_source_texture = if milky_way_path.is_file() {
        let sky_texture = asset_server.load("textures/milky_way_8k.jpg");
        let sky_mesh = sphere_mesh(&mut meshes, super::types::STARFIELD_RADIUS, 96);
        let sky_material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(sky_texture.clone()),
            emissive: LinearRgba::BLACK,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        commands.spawn((
            Mesh3d(sky_mesh),
            MeshMaterial3d(sky_material),
            Transform::default(),
            StarsBackdrop,
            NotShadowCaster,
        ));
        sky_texture
    } else {
        spawn_fallback_starfield(&mut commands, &mut meshes, &mut materials, &mut images)
    };

    commands.insert_resource(SkyEnvironmentState {
        source_equirect_texture: starfield_source_texture,
        generated_cubemap: None,
        applied_to_camera: false,
    });
}

pub(super) fn sync_environment_lighting_from_sky(
    mut commands: Commands,
    sky_environment: Option<ResMut<SkyEnvironmentState>>,
    mut images: ResMut<Assets<Image>>,
    camera_query: Query<Entity, With<MainCamera>>,
) {
    let Some(mut sky_environment) = sky_environment else {
        return;
    };
    if sky_environment.applied_to_camera {
        return;
    }

    if sky_environment.generated_cubemap.is_none() {
        let Some(source_image) = images.get(&sky_environment.source_equirect_texture) else {
            return;
        };

        let Some(cubemap_image) = equirectangular_to_cubemap_image(source_image, 256) else {
            return;
        };
        sky_environment.generated_cubemap = Some(images.add(cubemap_image));
    }

    let Some(cubemap_handle) = sky_environment.generated_cubemap.clone() else {
        return;
    };

    for camera_entity in &camera_query {
        commands
            .entity(camera_entity)
            .insert(GeneratedEnvironmentMapLight {
                environment_map: cubemap_handle.clone(),
                // Kept low so the solar key-light inverse-square falloff
                // creates a realistic brightness gradient across the solar system.
                intensity: 150.0,
                rotation: Quat::IDENTITY,
                affects_lightmapped_mesh_diffuse: true,
            });
    }

    sky_environment.applied_to_camera = true;
}

pub(super) fn start_horizons_sync(
    app_status: Res<AppStatus>,
    ephemeris: NonSend<EphemerisResource>,
    horizons_http_client: Option<Res<HorizonsHttpClient>>,
    mut horizons_sync: ResMut<HorizonsSyncState>,
) {
    *horizons_sync = HorizonsSyncState::new(BODIES.len());

    let _ = queue_horizons_sync_task(
        &app_status,
        &ephemeris,
        horizons_http_client.as_ref().map(|client| &client.client),
        &mut horizons_sync,
    );
}

pub(super) fn process_horizons_sync_requests(
    time: Res<Time>,
    app_status: Res<AppStatus>,
    ephemeris: NonSend<EphemerisResource>,
    horizons_http_client: Option<Res<HorizonsHttpClient>>,
    mut horizons_sync: ResMut<HorizonsSyncState>,
) {
    if horizons_sync.task.is_some() {
        return;
    }

    let now_seconds = time.elapsed_secs_f64();
    let scheduled_retry_due = should_trigger_scheduled_retry(&horizons_sync, now_seconds);
    let manual_retry_requested = horizons_sync.retry_requested;
    if !manual_retry_requested && !scheduled_retry_due {
        return;
    }

    if manual_retry_requested {
        horizons_sync.retry_attempt = 0;
    }
    horizons_sync.retry_requested = false;
    horizons_sync.next_retry_deadline_seconds = None;

    let _ = queue_horizons_sync_task(
        &app_status,
        &ephemeris,
        horizons_http_client.as_ref().map(|client| &client.client),
        &mut horizons_sync,
    );
}

pub(super) fn poll_horizons_sync_task(
    time: Res<Time>,
    mut horizons_sync: ResMut<HorizonsSyncState>,
) {
    let Some(task) = horizons_sync.task.as_mut() else {
        return;
    };

    let Some(result) = future::block_on(future::poll_once(task)) else {
        return;
    };

    horizons_sync.task = None;
    apply_horizons_sync_result(&mut horizons_sync, result, time.elapsed_secs_f64());

    eprintln!("{}", horizons_sync.status_line);
    for failure in &horizons_sync.failures {
        eprintln!("Horizons sync issue: {failure}");
    }
}

fn queue_horizons_sync_task(
    app_status: &AppStatus,
    ephemeris: &EphemerisResource,
    horizons_http_client: Option<&reqwest::blocking::Client>,
    horizons_sync: &mut HorizonsSyncState,
) -> bool {
    if !app_status.spice_enabled {
        horizons_sync.status_line = "Horizons sync disabled: SPICE mode not active".to_string();
        return false;
    }

    let Some(client) = horizons_http_client else {
        horizons_sync.status_line =
            "Horizons sync unavailable: could not initialize Horizons HTTP client".to_string();
        return false;
    };

    let utc_timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let target_count = BODIES
        .iter()
        .filter(|spec| !spec.spice_target.eq_ignore_ascii_case("SUN"))
        .count();

    let mut targets = Vec::new();
    let mut initial_failures = Vec::new();

    for (index, spec) in BODIES.iter().enumerate() {
        if spec.spice_target.eq_ignore_ascii_case("SUN") {
            continue;
        }

        let Some(command) = horizons_command_for_target(spec.spice_target) else {
            initial_failures.push(format!(
                "{}: no Horizons command mapping",
                spec.display_name
            ));
            continue;
        };

        let spice_au = ephemeris
            .ephemeris
            .position_au_at_utc_timestamp(spec.spice_target, &utc_timestamp);
        targets.push(HorizonsTargetSample {
            index,
            display_name: spec.display_name,
            command,
            spice_au,
        });
    }

    horizons_sync.status_line = format!(
        "Horizons sync in progress: {}/{} queued @ {utc_timestamp} UTC",
        targets.len(),
        target_count
    );
    horizons_sync.failures.clear();

    let input = HorizonsSyncTaskInput {
        utc_timestamp,
        target_count,
        targets,
        initial_failures,
    };

    let client = client.clone();
    let task_pool = AsyncComputeTaskPool::get();
    horizons_sync.task =
        Some(task_pool.spawn(async move { run_horizons_sync_task(client, input) }));

    true
}

fn apply_horizons_sync_result(
    horizons_sync: &mut HorizonsSyncState,
    result: HorizonsSyncResult,
    now_seconds: f64,
) {
    let HorizonsSyncResult {
        enabled,
        status_line,
        failures,
        per_body_au_offset,
    } = result;

    horizons_sync.enabled = enabled;
    horizons_sync.status_line = status_line;
    horizons_sync.failures = failures;
    horizons_sync.per_body_au_offset = per_body_au_offset;

    if enabled {
        horizons_sync.retry_attempt = 0;
        horizons_sync.next_retry_deadline_seconds = None;
        return;
    }

    if horizons_sync.retry_attempt >= HORIZONS_RETRY_MAX_ATTEMPTS {
        horizons_sync.next_retry_deadline_seconds = None;
        horizons_sync.status_line = format!(
            "{} | retries exhausted ({HORIZONS_RETRY_MAX_ATTEMPTS} attempts)",
            horizons_sync.status_line
        );
        return;
    }

    let delay_seconds = compute_retry_delay_seconds(horizons_sync.retry_attempt);
    horizons_sync.retry_attempt += 1;
    horizons_sync.next_retry_deadline_seconds = Some(now_seconds + delay_seconds);
    horizons_sync.status_line = format!(
        "{} | retrying in {:.1}s ({}/{})",
        horizons_sync.status_line,
        delay_seconds,
        horizons_sync.retry_attempt,
        HORIZONS_RETRY_MAX_ATTEMPTS
    );
}

fn compute_retry_delay_seconds(retry_attempt: u32) -> f64 {
    let delay = HORIZONS_RETRY_BASE_DELAY_SECS * 2_f64.powi(retry_attempt as i32);
    delay.min(HORIZONS_RETRY_MAX_DELAY_SECS)
}

fn should_trigger_scheduled_retry(horizons_sync: &HorizonsSyncState, now_seconds: f64) -> bool {
    matches!(
        horizons_sync.next_retry_deadline_seconds,
        Some(deadline) if now_seconds >= deadline
    )
}

pub(super) fn refresh_texture_status(
    asset_server: Res<AssetServer>,
    texture_registry: Option<Res<PlanetTextureRegistry>>,
    mut texture_status: ResMut<TextureStatus>,
) {
    let Some(texture_registry) = texture_registry else {
        return;
    };

    if texture_registry.entries.is_empty() {
        texture_status.summary = "Textures: 0/0 loaded".to_string();
        texture_status.failed.clear();
        return;
    }

    let mut loaded = 0usize;
    let mut failed = Vec::new();

    for entry in &texture_registry.entries {
        match asset_server.load_state(entry.handle.id()) {
            bevy::asset::LoadState::Loaded => loaded += 1,
            bevy::asset::LoadState::Failed(error) => {
                failed.push(format!("{} ({}): {}", entry.body_name, entry.path, error))
            }
            bevy::asset::LoadState::Loading | bevy::asset::LoadState::NotLoaded => {}
        }
    }

    let new_summary = format!(
        "Textures: {loaded}/{} loaded",
        texture_registry.entries.len()
    );

    if new_summary != texture_status.summary || failed != texture_status.failed {
        eprintln!("{new_summary}");
        for line in &failed {
            eprintln!("Texture load failed: {line}");
        }
    }

    texture_status.summary = new_summary;
    texture_status.failed = failed;
}

fn run_horizons_sync_task(
    client: reqwest::blocking::Client,
    input: HorizonsSyncTaskInput,
) -> HorizonsSyncResult {
    let mut failures = input.initial_failures;
    let mut per_body_au_offset = vec![DVec3::ZERO; BODIES.len()];
    let mut validated_count = 0usize;
    let mut max_delta_km = 0.0_f64;
    let mut max_delta_body = "N/A";

    for target in input.targets {
        match fetch_horizons_heliocentric_position_au_with_client(
            &client,
            target.command,
            &input.utc_timestamp,
        ) {
            Ok(horizons_au) => {
                let dx = horizons_au[0] - target.spice_au[0];
                let dy = horizons_au[1] - target.spice_au[1];
                let dz = horizons_au[2] - target.spice_au[2];

                let delta_km = (dx * dx + dy * dy + dz * dz).sqrt() * KM_PER_AU;
                if delta_km > max_delta_km {
                    max_delta_km = delta_km;
                    max_delta_body = target.display_name;
                }

                per_body_au_offset[target.index] = eclipj2000_to_scene([dx, dy, dz], 1.0);
                validated_count += 1;
            }
            Err(err) => {
                failures.push(format!("{}: {err}", target.display_name));
            }
        }
    }

    let enabled = validated_count > 0;
    let status_line = if enabled {
        if failures.is_empty() {
            format!(
                "Horizons sync active: {validated_count}/{} validated @ {} UTC (max Δ {:.0} km: {max_delta_body})",
                input.target_count, input.utc_timestamp, max_delta_km
            )
        } else {
            format!(
                "Horizons sync partial: {validated_count}/{} validated @ {} UTC ({} failed, max Δ {:.0} km: {max_delta_body})",
                input.target_count,
                input.utc_timestamp,
                failures.len(),
                max_delta_km
            )
        }
    } else {
        format!(
            "Horizons sync unavailable @ {} UTC: {}",
            input.utc_timestamp,
            failures
                .first()
                .cloned()
                .unwrap_or_else(|| "no targets validated".to_string())
        )
    };

    HorizonsSyncResult {
        enabled,
        status_line,
        failures,
        per_body_au_offset,
    }
}

pub(super) fn set_window_icon(windows: Query<Entity, With<Window>>) {
    let Ok(window_entity) = windows.single() else {
        return;
    };
    let small = match decode_icon(ICON_PNG_SMALL) {
        Some(v) => v,
        None => return,
    };
    let large = match decode_icon(ICON_PNG_LARGE) {
        Some(v) => v,
        None => return,
    };
    bevy::winit::WINIT_WINDOWS.with_borrow(|winit_windows| {
        if let Some(winit_window) = winit_windows.get_window(window_entity) {
            match winit::window::Icon::from_rgba(small.0, small.1, small.2) {
                Ok(icon) => winit_window.set_window_icon(Some(icon)),
                Err(err) => eprintln!("Failed to create window icon: {err}"),
            }
            #[cfg(target_os = "windows")]
            {
                use winit::platform::windows::WindowExtWindows;
                match winit::window::Icon::from_rgba(large.0, large.1, large.2) {
                    Ok(icon) => winit_window.set_taskbar_icon(Some(icon)),
                    Err(err) => eprintln!("Failed to create taskbar icon: {err}"),
                }
            }
            #[cfg(not(target_os = "windows"))]
            let _ = large;
        }
    });
}

fn decode_icon(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (w, h) = rgba.dimensions();
            Some((rgba.into_raw(), w, h))
        }
        Err(err) => {
            eprintln!("Failed to decode window icon: {err}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn compute_retry_delay_seconds_uses_exponential_backoff_with_cap() {
        assert_eq!(compute_retry_delay_seconds(0), 1.0);
        assert_eq!(compute_retry_delay_seconds(1), 2.0);
        assert_eq!(compute_retry_delay_seconds(2), 4.0);
        assert_eq!(
            compute_retry_delay_seconds(10),
            HORIZONS_RETRY_MAX_DELAY_SECS
        );
    }

    #[test]
    fn apply_horizons_sync_result_success_resets_retry_state() {
        let mut state = HorizonsSyncState::new(3);
        state.retry_attempt = 3;
        state.next_retry_deadline_seconds = Some(999.0);

        apply_horizons_sync_result(
            &mut state,
            HorizonsSyncResult {
                enabled: true,
                status_line: "ok".to_string(),
                failures: Vec::new(),
                per_body_au_offset: vec![DVec3::ZERO; 3],
            },
            100.0,
        );

        assert!(state.enabled);
        assert_eq!(state.retry_attempt, 0);
        assert!(state.next_retry_deadline_seconds.is_none());
    }

    #[test]
    fn apply_horizons_sync_result_failure_schedules_retry() {
        let mut state = HorizonsSyncState::new(2);

        apply_horizons_sync_result(
            &mut state,
            HorizonsSyncResult {
                enabled: false,
                status_line: "unavailable".to_string(),
                failures: vec!["earth failed".to_string()],
                per_body_au_offset: vec![DVec3::ZERO; 2],
            },
            10.0,
        );

        assert!(!state.enabled);
        assert_eq!(state.retry_attempt, 1);
        assert_eq!(state.next_retry_deadline_seconds, Some(11.0));
        assert!(state.status_line.contains("retrying in"));
    }

    #[test]
    fn apply_horizons_sync_result_failure_stops_after_max_attempts() {
        let mut state = HorizonsSyncState::new(1);
        state.retry_attempt = HORIZONS_RETRY_MAX_ATTEMPTS;

        apply_horizons_sync_result(
            &mut state,
            HorizonsSyncResult {
                enabled: false,
                status_line: "still unavailable".to_string(),
                failures: vec!["request failed".to_string()],
                per_body_au_offset: vec![DVec3::ZERO; 1],
            },
            20.0,
        );

        assert_eq!(state.retry_attempt, HORIZONS_RETRY_MAX_ATTEMPTS);
        assert!(state.next_retry_deadline_seconds.is_none());
        assert!(state.status_line.contains("retries exhausted"));
    }

    #[test]
    fn should_trigger_scheduled_retry_when_deadline_passed() {
        let mut state = HorizonsSyncState::new(1);
        state.next_retry_deadline_seconds = Some(5.0);
        assert!(!should_trigger_scheduled_retry(&state, 4.9));
        assert!(should_trigger_scheduled_retry(&state, 5.0));
    }

    #[test]
    fn run_horizons_sync_task_reports_unavailable_when_no_targets_validate() {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(10))
            .build()
            .expect("client");

        let result = run_horizons_sync_task(
            client,
            HorizonsSyncTaskInput {
                utc_timestamp: "2026-04-18 12:00:00".to_string(),
                target_count: 1,
                targets: Vec::new(),
                initial_failures: vec!["Earth: no mapping".to_string()],
            },
        );

        assert!(!result.enabled);
        assert!(result.status_line.contains("unavailable"));
        assert_eq!(result.failures.len(), 1);
    }
}
