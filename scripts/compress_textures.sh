#!/usr/bin/env bash
#
# Encode the downloaded planet/moon textures into GPU block-compressed,
# mipmapped KTX2. The format — and therefore the encoder — is chosen per
# platform:
#
#   - macOS / Apple Silicon (Metal): ASTC 4x4, via Khronos `ktx`
#   - Windows / Linux desktop GPUs:  BC7,      via AMD `compressonatorcli`
#
# Two encoders because AMD Compressonator ships no macOS build at all (the
# 4.5.x releases are Windows + Linux only), and Metal wants ASTC rather than
# BC7 anyway. Khronos KTX-Software covers macOS: install the signed package
# from https://github.com/KhronosGroup/KTX-Software/releases (grab
# KTX-Software-<ver>-Darwin-arm64.pkg — it puts `ktx` in /usr/local/bin).
#
# Each platform only ever holds its own .ktx2 set (textures are generated
# locally, never committed), and the loader is format-blind
# (util::resolve_texture_load_path picks .ktx2 -> .dds -> the .jpg), so the
# right format is simply selected here at encode time. Running this is an
# opt-in optimisation: block compression keeps textures ~4x smaller in VRAM
# than the RGBA8 the JPEGs decode to, and the embedded mip chain removes the
# shimmer you otherwise get on small/distant bodies.
#
# The 8K Milky Way backdrop is skipped: its pixels are read on the CPU to build
# the environment cubemap, which cannot come from a block-compressed image. The
# unused sun_2k_backup.jpg is skipped too.
#
# COLOUR NOTE: planet maps are sRGB base-colour textures. The ktx path says so
# explicitly (ASTC_4x4_SRGB_BLOCK + --assign-tf srgb). After encoding, verify
# in-app that colours look right; if they appear too dark, the output was
# tagged linear instead of sRGB and must be re-encoded.
#
# Note: exact Compressonator flags can vary by version — adjust dest_args below
# if your build rejects them.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
texture_dir="$script_dir/../assets/textures"

force=0
if [ "${1:-}" = "--force" ]; then
    force=1
fi

# Pick the encoder, and with it the block format this platform's GPU supports.
if [ "$(uname -s)" = "Darwin" ]; then
    encoder="ktx"
    fmt_label="ASTC 4x4"
    ext="ktx2"
    if ! command -v ktx >/dev/null 2>&1; then
        cat >&2 <<'EOF'
ktx not found on PATH. Install Khronos KTX-Software (AMD Compressonator has no
macOS build):

  1. Download KTX-Software-<version>-Darwin-arm64.pkg from
     https://github.com/KhronosGroup/KTX-Software/releases
  2. Open it (or: sudo installer -pkg <file>.pkg -target /)
  3. Re-run this script — `ktx` lands in /usr/local/bin.
EOF
        exit 1
    fi
else
    encoder="compressonatorcli"
    fmt_label="BC7"
    dest_args=(-fd BC7)
    if ! command -v compressonatorcli >/dev/null 2>&1; then
        echo "compressonatorcli not found on PATH. Install AMD Compressonator: https://gpuopen.com/compressonator/" >&2
        exit 1
    fi

    # Pick the output container. KTX2 is preferred, but some Compressonator
    # builds (notably the prebuilt Linux CLI packages) ship without a KTX2
    # writer and only emit DDS. Both containers carry raw BCn + mips, and the
    # loader (util::resolve_texture_load_path) reads .ktx2 -> .dds -> .jpg, so
    # DDS is an equivalent fallback. Probe once against the first source
    # instead of guessing.
    ext="ktx2"
    shopt -s nullglob
    probe_src=""
    for f in "$texture_dir"/*.png "$texture_dir"/*.jpg; do probe_src="$f"; break; done
    if [ -n "$probe_src" ]; then
        probe_out="$(mktemp -u).ktx2"
        if ! compressonatorcli "${dest_args[@]}" -miplevels 1 "$probe_src" "$probe_out" >/dev/null 2>&1 \
            || [ ! -f "$probe_out" ]; then
            ext="dds"
            echo "compressonatorcli cannot write .ktx2 on this build; falling back to .dds"
        fi
        rm -f "$probe_out"
    fi
fi
shopt -s nullglob

# BC7 and ASTC 4x4 both tile the image in 4x4 blocks, so the GPU rejects any
# texture whose width or height is not a multiple of 4 (wgpu panics with
# "Height N is not a multiple of Bc7RgbaUnormSrgb's block height (4)"). Source
# maps that ship with odd dimensions are rounded down to the nearest multiple
# of 4 at encode time. Dimensions are read via sips (macOS, built-in) or
# ImageMagick's identify (Linux); if neither is present we warn and encode
# as-is rather than risk a wrong resize.
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

# Convert a source into something the encoder will accept, when needed:
#
#   - `ktx create` reads EXR/PNG/raw but *not* JPEG, so every .jpg is decoded
#     to a temp PNG first (lossless — no second JPEG generation loss).
#   - odd-sized sources are shrunk to the nearest multiple of 4; neither CLI
#     has a resize flag (Compressonator verified against 4.5.52), so the resize
#     happens during that same PNG conversion.
#
# Echoes the path to feed the encoder: the original when nothing is needed,
# otherwise a temp PNG (caller deletes it).
prepare_source() {
    local f="$1" dims w h tw th tmp needs_png=0
    if [ "$encoder" = "ktx" ] && [ "${f##*.}" != "png" ]; then
        needs_png=1
    fi

    dims=$(image_dims "$f") || true
    if [ -z "${dims:-}" ]; then
        echo "  Could not read dimensions of $(basename "$f"); encoding without 4x4 block-alignment resize." >&2
        w=0; h=0; tw=0; th=0
    else
        w=${dims% *}
        h=${dims#* }
        tw=$(( w - w % 4 ))
        th=$(( h - h % 4 ))
    fi

    if [ "$tw" -eq "$w" ] && [ "$th" -eq "$h" ] && [ "$needs_png" -eq 0 ]; then
        printf '%s' "$f"
        return 0
    fi
    if [ "$tw" -ne "$w" ] || [ "$th" -ne "$h" ]; then
        echo "  $(basename "$f") is ${w}x${h}; resizing to ${tw}x${th} for 4x4 block alignment" >&2
    fi

    tmp="$(mktemp -u).png"
    if [ "$tw" -eq "$w" ] && [ "$th" -eq "$h" ]; then
        # Format conversion only.
        if command -v magick >/dev/null 2>&1; then
            magick "$f" "$tmp"
        elif command -v sips >/dev/null 2>&1; then
            sips -s format png "$f" --out "$tmp" >/dev/null
        else
            echo "  No image tool found (ImageMagick/sips) to convert $(basename "$f") to PNG." >&2
            return 1
        fi
    elif command -v magick >/dev/null 2>&1; then
        magick "$f" -resize "${tw}x${th}!" "$tmp"
    elif command -v convert >/dev/null 2>&1; then
        convert "$f" -resize "${tw}x${th}!" "$tmp"
    elif command -v sips >/dev/null 2>&1; then
        sips -z "$th" "$tw" -s format png "$f" --out "$tmp" >/dev/null
    else
        echo "  No resize tool found (ImageMagick/sips); encoding as-is — the GPU may reject this texture." >&2
        printf '%s' "$f"
        return 0
    fi
    printf '%s' "$tmp"
}

# Encode one prepared source into the compressed container.
encode() {
    local src="$1" out="$2"
    if [ "$encoder" = "ktx" ]; then
        # --assign-tf/--assign-primaries state what the planet maps actually
        # are (sRGB, BT.709) instead of letting ktx guess from absent PNG
        # metadata. --astc-quality medium is the codec default and encodes an
        # 8K map in a few seconds; raise it to `thorough` if you want to trade
        # minutes for a slightly better result.
        ktx create \
            --format ASTC_4x4_SRGB_BLOCK \
            --assign-tf srgb \
            --assign-primaries bt709 \
            --generate-mipmap \
            --astc-quality medium \
            "$src" "$out"
    else
        compressonatorcli "${dest_args[@]}" -miplevels 20 "$src" "$out" >/dev/null
    fi
}

# Textures that must NOT be compressed: the CPU-read backdrop, the unused
# lower-resolution sun backup, and the ring texture (setup.rs loads
# saturn_ring.png directly, not via resolve_texture_load_path, so a .ktx2
# would never be picked up).
skip="milky_way_8k.png sun_2k_backup.jpg saturn_ring.png"

for src in "$texture_dir"/*.jpg "$texture_dir"/*.png; do
    name="$(basename "$src")"
    case " $skip " in
        *" $name "*) continue ;;
    esac
    stem="${name%.*}"
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
    encode_src=$(prepare_source "$src")
    echo "Encoding $name -> $stem.$ext ($fmt_label + mipmaps)..."
    encode "$encode_src" "$out"
    if [ "$encode_src" != "$src" ]; then
        rm -f "$encode_src"
    fi
done

echo "Compressed ($fmt_label, .$ext) textures written to $texture_dir"
echo "The app automatically prefers the compressed files over the .jpg originals."
