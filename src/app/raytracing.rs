//! EXPERIMENTAL (local branch only): hardware-raytraced lighting via Bevy Solari.
//!
//! Built only with `--features raytracing`. Solari needs wgpu's
//! `EXPERIMENTAL_RAY_QUERY` plus bindless binding arrays; wgpu 30 exposes them on
//! Metal for Apple Silicon on macOS 15+. When the adapter lacks them Solari's
//! plugins log a warning and stay inert, and the app renders as usual.
//!
//! What it changes:
//! - The main camera gets `SolariLighting` (raytraced direct light with
//!   raytraced shadows, plus multi-bounce GI). `R` toggles it at runtime.
//! - Opaque, lit `StandardMaterial` meshes (planet/moon spheres, asteroids) get
//!   `RaytracingMesh3d`, so they enter the acceleration structure. Clouds
//!   (alpha-blended) and the unlit sky sphere stay out: Solari treats every
//!   hit as opaque, so a cloud shell or the sky sphere would block all light.
//! - While it is on, `sky_fill`'s shadow map is turned off (Solari's rays
//!   replace it). Solari 0.20 lights only from `DirectionalLight`s, emissive
//!   meshes in the acceleration structure and the camera's environment map, so
//!   `sky_fill` is the one light the bodies receive: the solar `PointLight` is
//!   ignored, and the Sun sphere is left out of the acceleration structure
//!   (below), so it lights nothing.
//! - The camera's settings differ from Solari's defaults (`solari_settings`):
//!   ReSTIR is on because macOS has no denoiser, and the world cache cells are
//!   shrunk to suit this scene's scale.
//! - `NotShadowCaster` meshes stay out of the acceleration structure too,
//!   matching the raster path. That matters for the Sun: `sky_fill` is aimed
//!   from the Sun at the camera's focus, so every shadow ray from the focused
//!   body toward that light runs straight into the Sun sphere, and with the
//!   Sun in the BVH the inspected planet rendered black.
//! - Solari scales *all* emitted light by the camera exposure (~0.001 at Bevy's
//!   default EV100 9.7), whereas the forward/deferred raster path leaves
//!   emissive unscaled (the deferred G-buffer even drops
//!   `emissive_exposure_weight`). So the Sun's glow and the sky sphere would
//!   render black under Solari. Unlit materials are turned into emissive ones
//!   once, and `sync_emissive_scale` multiplies every emissive by 1/exposure
//!   while Solari is on and divides it back out when `R` turns it off.

use super::render::apply_lighting_preset;
use super::types::{BODIES, LightingRig, MainCamera, SimulationState};
use bevy::camera::{CameraMainTextureUsages, Exposure};
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::render_resource::TextureUsages;
use bevy::render::renderer::RenderDevice;
use bevy::solari::SolariPlugins;
use bevy::solari::prelude::{RaytracingMesh3d, SolariLighting};

pub(super) struct RaytracingPlugin;

impl Plugin for RaytracingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SolariPlugins)
            // Per-pass GPU timings (timestamp queries) and frame time, logged
            // every 5 s, for comparing the two lighting paths: the 60 fps cap
            // hides the difference in the frame rate itself.
            .add_plugins((
                RenderDiagnosticsPlugin,
                FrameTimeDiagnosticsPlugin::default(),
                LogDiagnosticsPlugin {
                    wait_duration: std::time::Duration::from_secs(5),
                    ..default()
                },
            ))
            .add_systems(
                Startup,
                (report_adapter_support, disable_vsync_when_uncapped),
            )
            .add_systems(
                PostStartup,
                (enable_on_main_camera, convert_unlit_materials),
            )
            .add_systems(
                Update,
                (
                    attach_raytracing_meshes,
                    toggle_raytracing,
                    toggle_restir,
                    focus_body_from_env,
                    sync_emissive_scale.after(toggle_raytracing),
                    disable_shadow_maps_while_raytracing.after(apply_lighting_preset),
                ),
            );
    }
}

/// `SOLAR_NAVIGATOR_UNCAPPED=1` drops the frame cap and vsync so frame time
/// shows the real GPU cost. Off by default: see CLAUDE.md on what an uncapped
/// frame rate once did to a Windows/AMD machine.
pub(super) fn uncapped() -> bool {
    std::env::var_os("SOLAR_NAVIGATOR_UNCAPPED").is_some()
}

fn disable_vsync_when_uncapped(mut windows: Query<&mut Window>) {
    if uncapped() {
        for mut window in &mut windows {
            window.present_mode = bevy::window::PresentMode::AutoNoVsync;
        }
        info!("Raytracing: frame cap and vsync off (SOLAR_NAVIGATOR_UNCAPPED)");
    }
}

fn report_adapter_support(device: Res<RenderDevice>) {
    let required = SolariPlugins::required_wgpu_features();
    let features = device.features();
    if features.contains(required) {
        info!("Raytracing: adapter supports Solari's required features ({required:?})");
    } else {
        warn!(
            "Raytracing: adapter lacks {:?}; Solari stays inert",
            required.difference(features)
        );
    }
}

/// Solari's defaults, tuned for this app on Metal.
fn solari_settings() -> SolariLighting {
    SolariLighting {
        // Solari's only denoiser is DLSS Ray Reconstruction (NVIDIA), and with
        // ReSTIR off by default it relies on one; without it the macOS image is
        // raw path-traced noise. ReSTIR's sample reuse is the denoising we have.
        restir: true,
        // The default 0.15 is sized for metre-scale games, but Earth's visual
        // radius is 0.16 units here, so one cell would span a whole planet.
        // 0.005 is about half the smallest body (0.009); cells still grow with
        // distance from the camera (`world_cache_position_lod_scale`).
        world_cache_position_base_cell_size: 0.005,
        ..default()
    }
}

fn enable_on_main_camera(mut commands: Commands, camera: Single<Entity, With<MainCamera>>) {
    commands.entity(*camera).insert((
        solari_settings(),
        CameraMainTextureUsages::default().with(TextureUsages::STORAGE_BINDING),
    ));
}

/// Moves unlit colour into `emissive`, which both lighting paths display;
/// Solari ignores `unlit`. Every scene material exists by `PostStartup`.
fn convert_unlit_materials(mut materials: ResMut<Assets<StandardMaterial>>) {
    for (_, material) in materials.iter_mut() {
        if material.unlit {
            material.emissive = material.base_color.into();
            material.emissive_texture = material.base_color_texture.take();
            material.base_color = Color::BLACK;
            material.reflectance = 0.0;
            material.perceptual_roughness = 1.0;
            material.unlit = false;
        }
    }
}

/// Keeps emissive in step with the lighting path: scaled by 1/exposure while
/// Solari (which applies the exposure) is on, unscaled for the raster path.
fn sync_emissive_scale(
    camera: Single<Has<SolariLighting>, With<MainCamera>>,
    mut scaled: Local<bool>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let want_scaled = *camera;
    if *scaled == want_scaled {
        return;
    }
    let inverse_exposure = 1.0 / Exposure::default().exposure();
    let factor = if want_scaled {
        inverse_exposure
    } else {
        1.0 / inverse_exposure
    };
    for (_, material) in materials.iter_mut() {
        if material.emissive != LinearRgba::BLACK {
            material.emissive *= factor;
        }
    }
    *scaled = want_scaled;
}

/// Adds `RaytracingMesh3d` to opaque, lit, shadow-casting standard-material meshes, generating
/// the tangents Solari's BLAS builder requires (position/normal/uv/tangent).
#[allow(clippy::type_complexity)]
fn attach_raytracing_meshes(
    mut commands: Commands,
    candidates: Query<
        (Entity, &Mesh3d, &MeshMaterial3d<StandardMaterial>),
        (Without<RaytracingMesh3d>, Without<NotShadowCaster>),
    >,
    materials: Res<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, mesh_handle, material_handle) in &candidates {
        let Some(material) = materials.get(&material_handle.0) else {
            continue;
        };
        if material.unlit || !matches!(material.alpha_mode, AlphaMode::Opaque) {
            continue;
        }
        let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) else {
            continue;
        };
        if !mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT) && mesh.generate_tangents().is_err() {
            continue;
        }
        commands
            .entity(entity)
            .insert(RaytracingMesh3d(mesh_handle.0.clone()));
    }
}

fn toggle_raytracing(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Single<(Entity, Has<SolariLighting>), With<MainCamera>>,
) {
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    let (entity, enabled) = *camera;
    if enabled {
        commands.entity(entity).remove::<SolariLighting>();
        info!("Raytracing: off (deferred raster lighting)");
    } else {
        commands.entity(entity).insert(solari_settings());
        info!("Raytracing: on");
    }
}

/// `T` toggles ReSTIR, for measuring what it costs and how much noise it
/// removes without a denoiser.
fn toggle_restir(
    keys: Res<ButtonInput<KeyCode>>,
    mut camera: Single<Option<&mut SolariLighting>, With<MainCamera>>,
) {
    if !keys.just_pressed(KeyCode::KeyT) {
        return;
    }
    if let Some(solari) = camera.as_deref_mut() {
        solari.restir = !solari.restir;
        solari.reset = true;
        info!(
            "Raytracing: ReSTIR {}",
            if solari.restir { "on" } else { "off" }
        );
    }
}

/// `SOLAR_NAVIGATOR_FOCUS=<body name>` flies to that body shortly after
/// startup, so a measurement starts from the same close-up every run.
fn focus_body_from_env(mut frames: Local<u32>, mut simulation_state: ResMut<SimulationState>) {
    // Wait for the first ephemeris pass to place the bodies.
    const FOCUS_AFTER_FRAMES: u32 = 10;
    *frames += 1;
    if *frames != FOCUS_AFTER_FRAMES {
        return;
    }
    let Some(name) = std::env::var_os("SOLAR_NAVIGATOR_FOCUS") else {
        return;
    };
    let name = name.to_string_lossy();
    match BODIES
        .iter()
        .position(|spec| spec.display_name.eq_ignore_ascii_case(&name))
    {
        Some(index) => simulation_state.jump_request = Some(index),
        None => warn!("Raytracing: SOLAR_NAVIGATOR_FOCUS names no body: {name}"),
    }
}

fn disable_shadow_maps_while_raytracing(
    lighting_rig: Res<LightingRig>,
    camera: Single<Has<SolariLighting>, With<MainCamera>>,
    mut lights: Query<&mut DirectionalLight>,
) {
    if *camera && let Ok(mut sky_fill) = lights.get_mut(lighting_rig.sky_fill) {
        sky_fill.shadow_maps_enabled = false;
    }
}
