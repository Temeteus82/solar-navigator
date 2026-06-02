#!/usr/bin/env bash
#
# Encode the downloaded planet/moon textures into GPU block-compressed KTX2
# (BC7 + mipmaps) using AMD Compressonator.
#
# The app prefers a same-stem .ktx2 (or .dds) over the plain .jpg download at
# load time (see util::resolve_texture_load_path), so running this is purely an
# opt-in optimisation: it does not change which bodies render. BC7 keeps texture
# data block-compressed in VRAM (~4x smaller than the RGBA8 the JPEGs decode to)
# and the embedded mip chain removes the shimmer you otherwise get from
# un-mipmapped maps on small/distant bodies.
#
# Requires compressonatorcli on PATH: https://gpuopen.com/compressonator/
#
# The 8K Milky Way backdrop is skipped: its pixels are read on the CPU to build
# the environment cubemap, which cannot come from a block-compressed image. The
# unused sun_2k_backup.jpg is skipped too.
#
# COLOUR NOTE: planet maps are sRGB base-colour textures. After encoding, verify
# in-app that colours look right; if they appear too dark, the KTX2 was tagged
# linear instead of sRGB and must be re-encoded with an sRGB-aware setting.

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
    echo "Encoding $name -> ${name%.jpg}.ktx2 (BC7 + mipmaps)..."
    compressonatorcli -fd BC7 -miplevels 20 "$src" "$out" >/dev/null
done

echo "Compressed textures written to $texture_dir"
echo "The app automatically prefers the .ktx2 files over the .jpg originals."
