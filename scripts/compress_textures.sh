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

# Pick the output container. KTX2 is preferred, but some Compressonator builds
# (notably the prebuilt Linux CLI packages) ship without a KTX2 writer and only
# emit DDS. Both containers carry raw BCn/ASTC + mips, and the loader
# (util::resolve_texture_load_path) reads .ktx2 -> .dds -> .jpg, so DDS is an
# equivalent fallback. Probe once against the first source instead of guessing.
ext="ktx2"
shopt -s nullglob
probe_src=""
for f in "$texture_dir"/*.jpg; do probe_src="$f"; break; done
if [ -n "$probe_src" ]; then
    probe_out="$(mktemp -u).ktx2"
    if ! compressonatorcli "${dest_args[@]}" -miplevels 1 "$probe_src" "$probe_out" >/dev/null 2>&1 \
        || [ ! -f "$probe_out" ]; then
        ext="dds"
        echo "compressonatorcli cannot write .ktx2 on this build; falling back to .dds"
    fi
    rm -f "$probe_out"
fi

# BC7 and ASTC 4x4 both tile the image in 4x4 blocks, so the GPU rejects any
# texture whose width or height is not a multiple of 4 (wgpu panics with
# "Height N is not a multiple of Bc7RgbaUnormSrgb's block height (4)"). A few
# source maps ship with odd dimensions (e.g. Vesta's 4096x2047), so round them
# down to the nearest multiple of 4 at encode time. Dimensions are read via
# sips (macOS, built-in) or ImageMagick's identify (Linux); if neither is
# present we warn and encode as-is rather than risk a wrong resize.
image_dims() {
    local f="$1" out=""
    if command -v sips >/dev/null 2>&1; then
        out=$(sips -g pixelWidth -g pixelHeight "$f" 2>/dev/null \
            | awk '/pixelWidth/{w=$2}/pixelHeight/{h=$2}END{if(w&&h)print w, h}') || true
        if [ -n "$out" ]; then printf '%s' "$out"; return 0; fi
    fi
    if command -v magick >/dev/null 2>&1; then
        out=$(magick identify -format '%w %h' "$f" 2>/dev/null) || true
        if [ -n "$out" ]; then printf '%s' "$out"; return 0; fi
    fi
    if command -v identify >/dev/null 2>&1; then
        out=$(identify -format '%w %h' "$f" 2>/dev/null) || true
        if [ -n "$out" ]; then printf '%s' "$out"; return 0; fi
    fi
    return 1
}

# Echo the Compressonator -width/-height resize flags needed to make a source
# 4x4-block aligned, or nothing if it already is / its size can't be read.
block_align_args() {
    local f="$1" dims w h tw th
    dims=$(image_dims "$f") || true
    if [ -z "${dims:-}" ]; then
        echo "  Could not read dimensions of $(basename "$f"); encoding without 4x4 block-alignment resize." >&2
        return 0
    fi
    w=${dims% *}
    h=${dims#* }
    tw=$(( w - w % 4 ))
    th=$(( h - h % 4 ))
    if [ "$tw" -ne "$w" ] || [ "$th" -ne "$h" ]; then
        echo "  $(basename "$f") is ${w}x${h}; resizing to ${tw}x${th} for 4x4 block alignment" >&2
        printf -- '-width %s -height %s' "$tw" "$th"
    fi
}

# Textures that must NOT be compressed: the CPU-read backdrop and the unused
# lower-resolution sun backup.
skip="milky_way_8k.jpg sun_2k_backup.jpg"

for src in "$texture_dir"/*.jpg; do
    name="$(basename "$src")"
    case " $skip " in
        *" $name "*) continue ;;
    esac
    stem="${name%.jpg}"
    out="$texture_dir/$stem.$ext"
    # When we fall back to .dds, drop any stale same-stem .ktx2: the loader
    # prefers .ktx2 -> .dds, so a leftover .ktx2 from an earlier (KTX2-capable)
    # run would silently shadow the .dds we are writing now. Done before the
    # skip check so it is cleared even on no-op re-runs.
    if [ "$ext" = "dds" ] && [ -f "$texture_dir/$stem.ktx2" ]; then
        echo "Removing stale $stem.ktx2 (superseded by $stem.dds)"
        rm -f "$texture_dir/$stem.ktx2"
    fi
    if [ -f "$out" ] && [ "$force" -ne 1 ]; then
        echo "Skipping $stem.$ext (already present, use --force to re-encode)"
        continue
    fi
    align_args=$(block_align_args "$src")
    echo "Encoding $name -> $stem.$ext ($fmt_label + mipmaps)..."
    # align_args is intentionally unquoted so it word-splits into separate flags.
    # shellcheck disable=SC2086
    compressonatorcli "${dest_args[@]}" $align_args -miplevels 20 "$src" "$out" >/dev/null
done

echo "Compressed ($fmt_label, .$ext) textures written to $texture_dir"
echo "The app automatically prefers the compressed files over the .jpg originals."
