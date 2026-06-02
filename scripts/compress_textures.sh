#!/usr/bin/env bash
#
# Encode the downloaded planet/moon textures into GPU block-compressed,
# mipmapped KTX2 using AMD Compressonator. The format is chosen per platform:
#
#   - macOS / Apple Silicon (Metal): ASTC 4x4   (Metal supports ASTC, not BC7)
#   - Windows / Linux desktop GPUs:  BC7         (support BC7, not ASTC)
#
# Each platform only ever holds its own .ktx2 set (textures are generated
# locally, never committed), and the loader is format-blind
# (util::resolve_texture_load_path picks .ktx2 -> .dds -> the .jpg), so the
# right format is simply selected here at encode time. Running this is an
# opt-in optimisation: block compression keeps textures ~4x smaller in VRAM
# than the RGBA8 the JPEGs decode to, and the embedded mip chain removes the
# shimmer you otherwise get on small/distant bodies.
#
# Requires compressonatorcli on PATH: https://gpuopen.com/compressonator/
#
# The 8K Milky Way backdrop is skipped: its pixels are read on the CPU to build
# the environment cubemap, which cannot come from a block-compressed image. The
# unused sun_2k_backup.jpg is skipped too.
#
# COLOUR NOTE: planet maps are sRGB base-colour textures. After encoding, verify
# in-app that colours look right; if they appear too dark, the output was tagged
# linear instead of sRGB and must be re-encoded with an sRGB-aware setting.
#
# Note: exact Compressonator flags (e.g. the ASTC block-rate syntax) can vary by
# version — adjust DEST_ARGS below if your build rejects them.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
texture_dir="$script_dir/../assets/textures"

force=0
if [ "${1:-}" = "--force" ]; then
    force=1
fi

if ! command -v compressonatorcli >/dev/null 2>&1; then
    echo "compressonatorcli not found on PATH. Install AMD Compressonator: https://gpuopen.com/compressonator/" >&2
    exit 1
fi

# Pick the block-compression format supported by this platform's GPU.
if [ "$(uname -s)" = "Darwin" ]; then
    dest_args=(-fd ASTC -BlockRate 4x4)
    fmt_label="ASTC 4x4"
else
    dest_args=(-fd BC7)
    fmt_label="BC7"
fi

# Textures that must NOT be compressed: the CPU-read backdrop and the unused
# lower-resolution sun backup.
skip="milky_way_8k.jpg sun_2k_backup.jpg"

shopt -s nullglob
for src in "$texture_dir"/*.jpg; do
    name="$(basename "$src")"
    case " $skip " in
        *" $name "*) continue ;;
    esac
    out="$texture_dir/${name%.jpg}.ktx2"
    if [ -f "$out" ] && [ "$force" -ne 1 ]; then
        echo "Skipping ${name%.jpg}.ktx2 (already present, use --force to re-encode)"
        continue
    fi
    echo "Encoding $name -> ${name%.jpg}.ktx2 ($fmt_label + mipmaps)..."
    compressonatorcli "${dest_args[@]}" -miplevels 20 "$src" "$out" >/dev/null
done

echo "Compressed ($fmt_label) textures written to $texture_dir"
echo "The app automatically prefers the .ktx2 files over the .jpg originals."
