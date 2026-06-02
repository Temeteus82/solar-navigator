use super::types::{STARFIELD_COUNT, STARFIELD_RADIUS, StarPoint, StarsBackdrop};
use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::light::NotShadowCaster;
use bevy::math::DVec3;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use std::f32::consts::PI;
use std::path::{Path, PathBuf};

pub(super) fn sphere_mesh(
    meshes: &mut Assets<Mesh>,
    radius: f32,
    subdivisions: u32,
) -> Handle<Mesh> {
    let h_segments = subdivisions.max(16);
    let v_segments = (subdivisions / 2).max(8);
    meshes.add(Sphere::new(radius).mesh().uv(h_segments, v_segments))
}

pub(super) fn ring_mesh(
    meshes: &mut Assets<Mesh>,
    inner: f32,
    outer: f32,
    segments: u32,
) -> Handle<Mesh> {
    let n = segments as usize;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(2 * (n + 1));
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(2 * (n + 1));
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(2 * (n + 1));
    let mut indices: Vec<u32> = Vec::with_capacity(6 * n);

    for i in 0..=n {
        let theta = std::f32::consts::TAU * i as f32 / n as f32;
        let (sin_t, cos_t) = theta.sin_cos();
        positions.push([inner * cos_t, 0.0, inner * sin_t]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([i as f32 / n as f32, 0.0]);
        positions.push([outer * cos_t, 0.0, outer * sin_t]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([i as f32 / n as f32, 1.0]);
    }

    for i in 0..n {
        let i0 = (2 * i) as u32;
        let i1 = (2 * i + 1) as u32;
        let i2 = (2 * (i + 1)) as u32;
        let i3 = (2 * (i + 1) + 1) as u32;
        indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
}

/// 1x1 opaque-white texture used as a fallback when a custom material
/// requires a `Handle<Image>` but the on-disk asset is missing.
pub(super) fn white_pixel_image() -> Image {
    let mut image = Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![255, 255, 255, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

pub(super) fn spawn_fallback_starfield(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Handle<Image> {
    // Single draw-call fallback: generate an equirectangular star texture and map
    // it to an inside-out sky sphere instead of spawning hundreds of star meshes.
    let starfield_texture = images.add(generate_procedural_starfield_image(
        2048,
        1024,
        STARFIELD_COUNT,
    ));
    let sky_mesh = sphere_mesh(meshes, STARFIELD_RADIUS, 96);
    let sky_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(starfield_texture.clone()),
        emissive: LinearRgba::rgb(1.0, 1.0, 1.0),
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

    starfield_texture
}

pub(super) fn format_simulation_speed(multiplier: f64) -> String {
    if (multiplier - 1.0).abs() < 0.000_5 {
        "1.00x realtime".to_string()
    } else if multiplier >= 1000.0 {
        format!("{multiplier:.0}x realtime")
    } else if multiplier >= 10.0 {
        format!("{multiplier:.1}x realtime")
    } else {
        format!("{multiplier:.2}x realtime")
    }
}

pub(super) fn resolve_assets_root() -> PathBuf {
    if let Some(assets_root) = assets_root_from_env() {
        return assets_root;
    }

    if let Ok(exe_path) = std::env::current_exe()
        && let Some(assets_root) = assets_root_from_executable(&exe_path)
    {
        return assets_root;
    }

    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn assets_root_from_env() -> Option<PathBuf> {
    let configured = std::env::var_os("SOLAR_NAVIGATOR_ASSETS")?;
    let candidate = PathBuf::from(configured);
    candidate.is_dir().then_some(candidate)
}

fn assets_root_from_executable(exe_path: &Path) -> Option<PathBuf> {
    if let Some(bundle_assets) = macos_bundle_assets(exe_path)
        && bundle_assets.is_dir()
    {
        return Some(bundle_assets);
    }

    let exe_dir = exe_path.parent()?;
    let candidates = [
        exe_dir.join("assets"),
        exe_dir.join("..").join("assets"),
        exe_dir.join("..").join("..").join("assets"),
        exe_dir
            .join("..")
            .join("share")
            .join("solar-navigator")
            .join("assets"),
    ];

    candidates.into_iter().find(|candidate| candidate.is_dir())
}

#[cfg(target_os = "macos")]
fn macos_bundle_assets(exe_path: &Path) -> Option<PathBuf> {
    let macos_dir = exe_path.parent()?;
    if macos_dir.file_name().and_then(|name| name.to_str()) != Some("MacOS") {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    Some(contents_dir.join("Resources").join("assets"))
}

#[cfg(not(target_os = "macos"))]
fn macos_bundle_assets(_exe_path: &Path) -> Option<PathBuf> {
    None
}

pub(super) fn color_from_rgba(value: [f32; 4]) -> Color {
    Color::srgba(value[0], value[1], value[2], value[3])
}

pub(super) fn linear_from_rgb(value: [f32; 3]) -> LinearRgba {
    LinearRgba::rgb(value[0], value[1], value[2])
}

pub(super) fn equirectangular_to_cubemap_image(source: &Image, face_size: u32) -> Option<Image> {
    if face_size == 0 {
        return None;
    }

    let (src_width, src_height, src_data) = image_to_rgba8_data(source)?;
    if src_width == 0 || src_height == 0 {
        return None;
    }

    let face_pixel_count = (face_size * face_size) as usize;
    let mut cubemap_data = vec![0_u8; face_pixel_count * 6 * 4];

    for face in 0..6_u32 {
        for y in 0..face_size {
            for x in 0..face_size {
                let s = (2.0 * (x as f32 + 0.5) / face_size as f32) - 1.0;
                let t = (2.0 * (y as f32 + 0.5) / face_size as f32) - 1.0;
                let direction = cubemap_face_direction(face, s, t).normalize_or_zero();

                let u = (0.5 + direction.z.atan2(direction.x) / (2.0 * PI)).rem_euclid(1.0);
                let v = (0.5 - direction.y.asin() / PI).clamp(0.0, 1.0);

                let src_x = ((u * src_width as f32) as u32).min(src_width.saturating_sub(1));
                let src_y = ((v * src_height as f32) as u32).min(src_height.saturating_sub(1));
                let src_index = (src_y as usize * src_width as usize + src_x as usize) * 4;

                let dst_layer_row = face * face_size + y;
                let dst_index = (dst_layer_row as usize * face_size as usize + x as usize) * 4;
                cubemap_data[dst_index..dst_index + 4]
                    .copy_from_slice(&src_data[src_index..src_index + 4]);
            }
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: face_size,
            height: face_size * 6,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        cubemap_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image.reinterpret_stacked_2d_as_array(6).ok()?;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });

    Some(image)
}

fn generate_procedural_starfield_image(width: u32, height: u32, star_count: usize) -> Image {
    let mut data = vec![0_u8; (width * height * 4) as usize];

    for y in 0..height {
        let v = y as f32 / (height.saturating_sub(1).max(1)) as f32;
        let vertical_tint = 0.004 + (0.5 - (v - 0.5).abs()) * 0.01;
        let base_r = (vertical_tint * 255.0) as u8;
        let base_g = ((vertical_tint * 1.8) * 255.0) as u8;
        let base_b = ((vertical_tint * 3.8) * 255.0) as u8;

        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            data[index] = base_r;
            data[index + 1] = base_g;
            data[index + 2] = base_b;
            data[index + 3] = 255;
        }
    }

    for point in generate_starfield(star_count) {
        let direction = point.position.normalize_or_zero();
        let u = (0.5 + direction.z.atan2(direction.x) / (2.0 * PI)).rem_euclid(1.0);
        let v = (0.5 - direction.y.asin() / PI).clamp(0.0, 1.0);

        let cx = (u * width as f32) as i32;
        let cy = (v * height as f32) as i32;
        let radius_px = (point.size * 1.35).clamp(1.0, 4.0);
        draw_star_splat(
            &mut data,
            width,
            height,
            cx,
            cy,
            radius_px,
            [point.color[0], point.color[1], point.color[2]],
        );
    }

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

fn draw_star_splat(
    data: &mut [u8],
    width: u32,
    height: u32,
    cx: i32,
    cy: i32,
    radius: f32,
    color: [f32; 3],
) {
    let radius_i = radius.ceil() as i32;
    let sigma2 = (radius * 0.55).max(0.2).powi(2);

    for oy in -radius_i..=radius_i {
        let y = cy + oy;
        if y < 0 || y >= height as i32 {
            continue;
        }

        for ox in -radius_i..=radius_i {
            let x_unwrapped = cx + ox;
            let x = x_unwrapped.rem_euclid(width as i32);

            let dist2 = (ox * ox + oy * oy) as f32;
            let falloff = (-dist2 / (2.0 * sigma2)).exp();
            if falloff < 0.01 {
                continue;
            }

            let index = ((y as u32 * width + x as u32) * 4) as usize;
            for channel in 0..3 {
                let existing = data[index + channel] as f32 / 255.0;
                let added = color[channel] * falloff * 1.5;
                data[index + channel] = ((existing + added).clamp(0.0, 1.0) * 255.0) as u8;
            }
            data[index + 3] = 255;
        }
    }
}

fn image_to_rgba8_data(image: &Image) -> Option<(u32, u32, Vec<u8>)> {
    let converted = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => image.clone(),
        _ => image.convert(TextureFormat::Rgba8UnormSrgb)?,
    };

    let data = converted.data.clone()?;
    Some((converted.width(), converted.height(), data))
}

fn cubemap_face_direction(face: u32, s: f32, t: f32) -> Vec3 {
    match face {
        // +X
        0 => Vec3::new(1.0, -t, -s),
        // -X
        1 => Vec3::new(-1.0, -t, s),
        // +Y
        2 => Vec3::new(s, 1.0, t),
        // -Y
        3 => Vec3::new(s, -1.0, -t),
        // +Z
        4 => Vec3::new(s, -t, 1.0),
        // -Z
        _ => Vec3::new(-s, -t, -1.0),
    }
}

fn generate_starfield(count: usize) -> Vec<StarPoint> {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut stars = Vec::with_capacity(count);

    for _ in 0..count {
        let u = random01(&mut seed);
        let v = random01(&mut seed);
        let theta = std::f32::consts::TAU * u;
        let z = 2.0 * v - 1.0;
        let radial = (1.0 - z * z).max(0.0).sqrt();

        let direction = Vec3::new(radial * theta.cos(), z, radial * theta.sin());
        let radius = STARFIELD_RADIUS * (0.72 + 0.28 * random01(&mut seed));

        let tint = random01(&mut seed);
        let color = if tint < 0.45 {
            [0.86, 0.9, 1.0, 1.0]
        } else if tint < 0.85 {
            [0.98, 0.98, 0.98, 1.0]
        } else {
            [1.0, 0.92, 0.82, 1.0]
        };

        let size = 0.8 + random01(&mut seed) * random01(&mut seed) * 2.2;
        stars.push(StarPoint {
            position: direction * radius,
            color,
            size,
        });
    }

    stars
}

/// Shared deterministic LCG (Knuth MMIX constants) returning a value in
/// `[0, 1]`. Used by both the procedural starfield and the asteroid belt so a
/// single seeded sequence is reproducible across the codebase.
pub(super) fn random01(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 32) as u32) as f32 / u32::MAX as f32
}

/// Pick the best on-disk variant for a configured texture filename, preferring
/// a same-stem GPU-compressed container (block-compressed + mipmapped) over the
/// original image. Order: `.ktx2` → `.dds` → the configured file. Returns the
/// asset-relative path to load (e.g. `"textures/earth.ktx2"`), or `None` if no
/// variant is present on disk.
///
/// This lets the renderer transparently use compressed textures when they have
/// been generated (see `scripts/compress_textures.*`) while still working with
/// the plain `.jpg`/`.png` downloads when they have not.
pub(super) fn resolve_texture_load_path(texture_dir: &Path, file: &str) -> Option<String> {
    if let Some(stem) = Path::new(file).file_stem().and_then(|s| s.to_str()) {
        for ext in ["ktx2", "dds"] {
            let candidate = format!("{stem}.{ext}");
            if texture_dir.join(&candidate).is_file() {
                return Some(format!("textures/{candidate}"));
            }
        }
    }
    if texture_dir.join(file).is_file() {
        return Some(format!("textures/{file}"));
    }
    None
}

/// Convert a heliocentric position from the ECLIPJ2000 frame (SPICE convention,
/// Z = ecliptic north, right-handed) to Bevy scene space (Y-up, right-handed).
///
/// The mapping is: scene X = spice X, scene Y = spice Z, scene Z = –spice Y.
/// `scale` is multiplied into every component (e.g. AU_TO_SCENE_UNITS).
pub(super) fn eclipj2000_to_scene(au: [f64; 3], scale: f64) -> DVec3 {
    DVec3::new(au[0] * scale, au[2] * scale, -au[1] * scale)
}

#[cfg(test)]
mod tests {
    use super::{
        assets_root_from_executable, equirectangular_to_cubemap_image, format_simulation_speed,
        resolve_texture_load_path,
    };
    use bevy::asset::RenderAssetUsages;
    use bevy::image::Image;
    use bevy::render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDimension,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn format_simulation_speed_formats_near_realtime_with_two_decimals() {
        assert_eq!(format_simulation_speed(1.0), "1.00x realtime");
        assert_eq!(format_simulation_speed(1.0004), "1.00x realtime");
        assert_eq!(format_simulation_speed(9.876), "9.88x realtime");
    }

    #[test]
    fn format_simulation_speed_formats_tens_with_one_decimal() {
        assert_eq!(format_simulation_speed(10.25), "10.2x realtime");
        assert_eq!(format_simulation_speed(542.67), "542.7x realtime");
    }

    #[test]
    fn format_simulation_speed_formats_thousands_without_decimals() {
        assert_eq!(format_simulation_speed(1000.0), "1000x realtime");
        assert_eq!(format_simulation_speed(1234.5), "1234x realtime");
    }

    #[test]
    fn equirectangular_conversion_generates_cube_view() {
        let source = Image::new(
            Extent3d {
                width: 4,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![
                255, 0, 0, 255, 255, 128, 0, 255, 255, 255, 0, 255, 0, 255, 0, 255, 0, 255, 255,
                255, 0, 128, 255, 255, 0, 0, 255, 255, 128, 0, 255, 255,
            ],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );

        let cubemap = equirectangular_to_cubemap_image(&source, 4)
            .expect("expected equirectangular image to convert");
        assert_eq!(cubemap.width(), 4);
        assert_eq!(cubemap.height(), 4);
        assert_eq!(cubemap.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(
            cubemap
                .texture_view_descriptor
                .as_ref()
                .and_then(|descriptor| descriptor.dimension),
            Some(TextureViewDimension::Cube)
        );
    }

    #[test]
    fn assets_root_from_executable_finds_sibling_assets_directory() {
        let temp_root = unique_temp_path("assets-root");
        let binary_dir = temp_root.join("bin");
        let assets_dir = binary_dir.join("assets");
        fs::create_dir_all(&assets_dir).expect("failed to create test assets directory");

        let executable = binary_dir.join("solar-navigator");
        let resolved = assets_root_from_executable(&executable);
        assert_eq!(resolved, Some(assets_dir));

        let _ = fs::remove_dir_all(&temp_root);
    }

    #[test]
    fn resolve_texture_load_path_prefers_compressed_variants() {
        let root = unique_temp_path("texture-resolve");
        fs::create_dir_all(&root).expect("create temp texture dir");

        // Only the original present → use it.
        fs::write(root.join("earth.jpg"), b"jpg").unwrap();
        assert_eq!(
            resolve_texture_load_path(&root, "earth.jpg"),
            Some("textures/earth.jpg".to_string())
        );

        // A .dds is preferred over the plain image.
        fs::write(root.join("earth.dds"), b"dds").unwrap();
        assert_eq!(
            resolve_texture_load_path(&root, "earth.jpg"),
            Some("textures/earth.dds".to_string())
        );

        // A .ktx2 is preferred over everything else.
        fs::write(root.join("earth.ktx2"), b"ktx2").unwrap();
        assert_eq!(
            resolve_texture_load_path(&root, "earth.jpg"),
            Some("textures/earth.ktx2".to_string())
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_texture_load_path_returns_none_when_absent() {
        let root = unique_temp_path("texture-missing");
        fs::create_dir_all(&root).expect("create temp texture dir");
        assert_eq!(resolve_texture_load_path(&root, "venus.jpg"), None);
        let _ = fs::remove_dir_all(&root);
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "solar-navigator-{prefix}-{}-{nanos}",
            std::process::id()
        ))
    }
}
