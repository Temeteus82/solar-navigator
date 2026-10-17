//! Encode the downloaded planet/moon textures in `assets/textures/` into GPU
//! block-compressed, mipmapped KTX2 next to their sources:
//!
//! ```text
//! cargo run --release --example compress_textures --features compress_textures [-- --force]
//! ```
//!
//! The block format is the one this platform's GPU samples: ASTC 4x4 on macOS
//! (Apple Silicon / Metal has no BC7) and BC7 everywhere else. Encoding is done
//! in-process by `ctt`, the library behind Bevy 0.20's `CompressedImageSaver`;
//! it compiles its C++ encoders with the system C++ compiler, which is why it
//! sits behind the opt-in `compress_textures` feature and never touches the
//! app's own (portable) build. `util::resolve_texture_load_path` prefers the
//! `.ktx2` over the `.png`/`.jpg` automatically.
//!
//! The 8K Milky Way backdrop is skipped: its pixels are read on the CPU to build
//! the environment cubemap, and Bevy can only convert uncompressed formats back
//! to RGBA8 (`Image::convert` covers R8/Rg8/Rgba8/Bgra8). A block-compressed
//! backdrop makes that conversion return `None`, and
//! `sync_environment_lighting_from_sky` then silently retries every frame
//! without ever applying the environment map.

use ctt::{
    AlphaMode, ColorSpace, Container, ConvertSettings, Format, Image, MipmapFilter, PipelineOutput,
    Quality, Surface, TargetFormat, TextureKind, encoders::Encoder,
};
use image::imageops::FilterType;
use std::{path::Path, time::Instant};

const SKIP: &[&str] = &["milky_way_8k.png"];

#[cfg(target_os = "macos")]
const BLOCK_FORMAT: (Format, &str) = (Format::ASTC_4x4_UNORM_BLOCK, "ASTC 4x4");
#[cfg(not(target_os = "macos"))]
const BLOCK_FORMAT: (Format, &str) = (Format::BC7_UNORM_BLOCK, "BC7");

fn main() {
    let force = std::env::args().any(|arg| arg == "--force");
    let texture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/textures");
    let mut sources: Vec<_> = std::fs::read_dir(&texture_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", texture_dir.display()))
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            matches!(ext, "png" | "jpg") && !SKIP.contains(&name)
        })
        .collect();
    sources.sort();

    let (format, label) = BLOCK_FORMAT;
    let started = Instant::now();
    for src in sources {
        let out = src.with_extension("ktx2");
        let name = src.file_name().unwrap().to_string_lossy();
        if out.is_file() && !force {
            println!(
                "Skipping {} (already present, pass --force to re-encode)",
                out.display()
            );
            continue;
        }
        let file_started = Instant::now();
        let mut rgba = image::open(&src)
            .unwrap_or_else(|e| panic!("cannot decode {name}: {e}"))
            .to_rgba8();

        // BC7 and ASTC 4x4 tile the image in 4x4 blocks, and wgpu rejects a
        // compressed texture whose size is not a multiple of the block size
        // ("Height N is not a multiple of Bc7RgbaUnormSrgb's block height").
        // Some sources ship odd sizes (Vesta's 2405x1202), so shrink those to
        // the nearest multiple of 4.
        let (w, h) = rgba.dimensions();
        let (aligned_w, aligned_h) = (w - w % 4, h - h % 4);
        if (aligned_w, aligned_h) != (w, h) {
            println!(
                "  {name} is {w}x{h}; resizing to {aligned_w}x{aligned_h} for 4x4 block alignment"
            );
            rgba = image::imageops::resize(&rgba, aligned_w, aligned_h, FilterType::Lanczos3);
        }

        let (w, h) = rgba.dimensions();
        let image = Image {
            surfaces: vec![vec![Surface {
                data: rgba.into_raw(),
                width: w,
                height: h,
                depth: 1,
                stride: w * 4,
                slice_stride: 0,
                format: Format::R8G8B8A8_SRGB,
                color_space: ColorSpace::Srgb,
                alpha: AlphaMode::Straight,
            }]],
            kind: TextureKind::Texture2D,
        };
        // Planet maps are sRGB base colour; ctt tags the output `_SRGB_BLOCK`
        // from the input colour space. Alpha stays straight: only the Saturn
        // ring map has meaningful alpha, and its material blends straight alpha.
        let settings = ConvertSettings {
            format: Some(TargetFormat::Compressed {
                format,
                encoder: Encoder::Auto,
            }),
            container: Container::ktx2_zstd(0),
            // A one-off offline encode: take the best preset (~3 s per 8K map
            // on 32 threads).
            quality: Quality::VerySlow,
            output_color_space: None,
            output_alpha: None,
            allow_discarding_alpha: false,
            swizzle: None,
            mipmap: true,
            mipmap_count: None,
            mipmap_filter: MipmapFilter::Lanczos3,
        };
        let PipelineOutput::Encoded(bytes) =
            ctt::convert(image, settings).unwrap_or_else(|e| panic!("cannot encode {name}: {e}"))
        else {
            unreachable!("a Ktx2 container always yields encoded bytes");
        };
        std::fs::write(&out, &bytes)
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));
        println!(
            "Encoded {name} -> {} ({label} + mipmaps, {:.1} MB, {:.1} s)",
            out.file_name().unwrap().to_string_lossy(),
            bytes.len() as f64 / 1e6,
            file_started.elapsed().as_secs_f64()
        );
    }
    println!("Done in {:.1} s", started.elapsed().as_secs_f64());
}
