#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEXTURE_DIR="${ROOT_DIR}/assets/textures"
mkdir -p "${TEXTURE_DIR}"

# FULL_RES=1 downloads heavy science products (hundreds of MB each) and
# downsamples them to TARGET_WIDTH for runtime usage. Everything is written as
# lossless PNG.
FULL_RES="${FULL_RES:-0}"
TARGET_WIDTH="${TARGET_WIDTH:-4096}"

# Existing textures are left alone unless --force is passed, so adding one body
# does not re-fetch (or clobber) the rest. Matches the PowerShell port's -Force.
FORCE=0
if [[ "${1:-}" == "--force" ]]; then
  FORCE=1
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

# Downloads into a private temp *directory* so the file can keep a real
# extension. BSD mktemp only substitutes a trailing run of Xs, so the obvious
# "...-XXXXXX.${suffix}" template is taken literally on macOS: the first run
# creates that exact name and every later run fails with "File exists" until
# the leftover is deleted by hand. The caller removes the whole directory.
fetch_to_tmp() {
  local url="$1"
  local suffix="$2"
  local dir
  dir="$(mktemp -d "${TMPDIR:-/tmp}/science-texture-XXXXXX")"
  curl -fL --retry 3 --retry-delay 1 "$url" -o "${dir}/download.${suffix}"
  printf "%s" "${dir}/download.${suffix}"
}

# sips is macOS-only, so requiring it outright made this script impossible to
# run on Linux — it exited at the dependency check before fetching anything.
# ImageMagick covers every supported platform; sips stays last so a stock macOS
# box still works with nothing installed.
# `convert` is also the name of Windows' filesystem conversion tool, which sits
# in PATH under Git Bash and answers -version with "Invalid drive
# specification". Probe for the ImageMagick banner so that impostor is never
# selected — otherwise every re-encode fails after the download has already
# succeeded.
check_candidate() {
  command -v "$1" >/dev/null 2>&1 || return 1
  [[ "$1" != "convert" ]] || "$1" -version 2>&1 | grep -qi imagemagick
}

RESIZER=""
for candidate in magick convert sips; do
  if check_candidate "$candidate"; then
    RESIZER="$candidate"
    break
  fi
done

# Always writes lossless PNG — every destination in this script is .png.
# Mirrors the PowerShell port's Save-Resized.
convert_image() {
  local src="$1"
  local dest="$2"
  # Optional exact height. Sources whose aspect is not exactly 2:1 need to be
  # squared up rather than merely width-scaled — see the Vesta note below.
  local exact_height="${3:-}"

  case "$RESIZER" in
    magick | convert)
      # Trailing "!" ignores aspect ratio, matching the sips -z behaviour below.
      local geometry="${TARGET_WIDTH}x"
      if [[ -n "$exact_height" ]]; then
        geometry="${TARGET_WIDTH}x${exact_height}!"
      fi
      # ImageMagick's PNG coder picks its own colour type on write and collapses
      # single-channel sources (Ceres, the greyscale Europa/Callisto mosaics,
      # Pluto, Charon) to an 8-bit grey PNG — a different pixel format than what
      # compress_textures.* hands the BC7/ASTC encoders. The PNG32: prefix pins
      # 8-bit RGBA; -type TrueColor does not, because the coder re-detects
      # afterwards.
      "$RESIZER" "$src" -resize "$geometry" -colorspace sRGB "PNG32:${dest}"
      ;;
    sips)
      if [[ -n "$exact_height" ]]; then
        sips -z "$exact_height" "${TARGET_WIDTH}" --setProperty format png "$src" --out "$dest" >/dev/null
      else
        sips --resampleWidth "${TARGET_WIDTH}" --setProperty format png "$src" --out "$dest" >/dev/null
      fi
      ;;
  esac
}

fetch_and_convert() {
  local url="$1"
  local src_ext="$2"
  local dest="$3"
  local exact_height="${4:-}"

  if [[ -f "$dest" && "$FORCE" != "1" ]]; then
    echo "Skipping $(basename "$dest") (already present, use --force to re-download)"
    return 0
  fi

  local tmp
  tmp="$(fetch_to_tmp "$url" "$src_ext")"
  trap 'rm -rf "$(dirname "$tmp")"' RETURN

  # Every destination is .png, so even the fast-mode browse renderings — already
  # JPEG at ~1024 wide — are re-encoded rather than copied: a .png must never end
  # up holding JPEG bytes. The full-res sources are tens of thousands of pixels
  # wide and are brought down to TARGET_WIDTH on the way through.
  convert_image "$tmp" "$dest" "$exact_height"

  chmod 0644 "$dest"
}

require_cmd curl
if [[ -z "$RESIZER" ]]; then
  echo "Missing an image resizer: install ImageMagick (magick/convert), or run on macOS where sips is built in." >&2
  exit 1
fi

# Ceres comes from the USGS Astrogeology mosaic archive (served from the
# asc-pds-services S3 bucket): the Dawn FC clear-filter global mosaic at 20 ppd,
# 7383x3691. It is the source of ceres.png and is used in both
# modes, since USGS publishes no smaller browse rendering of it.
CERES_URL='https://asc-pds-services.s3.us-west-2.amazonaws.com/mosaic/Ceres_Dawn_FC_DLR_global_20ppd_Oct2015.tif'

# Vesta's truecolor mosaic exists only on DLR's Dawn GIS server, which is alive
# and serving (verified 2026-07-31 — an earlier note in this project claiming it
# had stopped responding was wrong). Two products are published, and only one is
# usable here:
#
#   Vesta_true_color_HAMO-1-2_global.png   26704x13080   equirectangular
#   Vesta_true_color_HAMO-1-2.png           2405x1202    MOLLWEIDE
#
# The compact one is an equal-area ellipse with black corners. Mapped onto a UV
# sphere it crushes the terrain into a band and smears the projection's dead
# corners across the globe — that is exactly how the broken map replaced in PR
# #65 got committed. So Vesta is fetched only in FULL_RES mode, from the global
# product, which is a 357 MB download. Nothing under assets/textures/ is stored
# in git, so fast mode simply leaves vesta.png absent and the body renders in
# its fallback colour.
#
# That mosaic is 26704x13080 (~2.042:1), not 2:1, so it is squared up to an
# exact 4096x2048 rather than merely width-scaled: the equirectangular mapping
# assumes 2:1, and 4096x2006 would leave a height that is not 4-aligned, which
# compress_textures.sh would then shrink to 2004 and drift further. Same
# normalisation applied in PR #65.
VESTA_GLOBAL_URL='https://dawngis.dlr.de/data/Vesta/mosaics/HAMO/truecolor/Vesta_true_color_HAMO-1-2_global.png'

if [[ "$FULL_RES" == "1" ]]; then
  echo "Downloading FULL-RES science mosaics (large files)..."
  fetch_and_convert "$CERES_URL" "tif" "${TEXTURE_DIR}/ceres.png"
  fetch_and_convert "$VESTA_GLOBAL_URL" "png" "${TEXTURE_DIR}/vesta.png" 2048
  fetch_and_convert \
    "https://planetarymaps.usgs.gov/mosaic/Pluto_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif" \
    "tif" \
    "${TEXTURE_DIR}/pluto.png"
  fetch_and_convert \
    "https://planetarymaps.usgs.gov/mosaic/Charon_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif" \
    "tif" \
    "${TEXTURE_DIR}/charon.png"
  # Galilean moons, USGS Astrogeology global mosaics. Io and Ganymede have
  # published colour-merge products; Europa and Callisto exist only as
  # greyscale mosaics there, so those two render monochrome (no colour
  # equirectangular map of them is published by USGS).
  fetch_and_convert \
    "https://planetarymaps.usgs.gov/mosaic/Io_Galileo_SSI_Global_Mosaic_ClrMerge_1km.tif" \
    "tif" \
    "${TEXTURE_DIR}/io.png"
  fetch_and_convert \
    "https://planetarymaps.usgs.gov/mosaic/Europa_Voyager_GalileoSSI_global_mosaic_500m.tif" \
    "tif" \
    "${TEXTURE_DIR}/europa.png"
  fetch_and_convert \
    "https://planetarymaps.usgs.gov/mosaic/Ganymede_Voyager_GalileoSSI_Global_ClrMosaic_1435m.tif" \
    "tif" \
    "${TEXTURE_DIR}/ganymede.png"
  fetch_and_convert \
    "https://planetarymaps.usgs.gov/mosaic/Callisto_Voyager_GalileoSSI_global_mosaic_1km.tif" \
    "tif" \
    "${TEXTURE_DIR}/callisto.png"
else
  echo "Downloading compact science textures (fast mode)..."
  fetch_and_convert "$CERES_URL" "tif" "${TEXTURE_DIR}/ceres.png"
  echo "Skipping vesta.png (only the Mollweide-projected preview is published at"
  echo "  this size, and it maps onto a sphere wrong. Vesta renders in its fallback"
  echo "  colour until you re-run with FULL_RES=1 for the 357 MB equirectangular mosaic.)"
  fetch_and_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/a5f1b7f4-9822-4697-a201-e23ef4bd3e16/resource/96be2aa1-f384-4a9f-9458-a8431a0e7956/download/pluto_newhorizons_global_mosaic_300m_jul2017_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/pluto.png"
  fetch_and_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/93827f6c-8feb-42b6-98e6-b0ce57c7d2c8/resource/1abf318c-3290-4aa0-932e-a34f32d7f6ad/download/charon_newhorizons_global_mosaic_300m_jul2017_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/charon.png"
  # 1024-wide browse renderings of the same USGS mosaics used above.
  fetch_and_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/0fc15885-24ee-4d9d-9666-11de0667c10c/resource/73d4c1f7-8c07-4b28-90ea-f47f7531c5ca/download/full.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/io.png"
  fetch_and_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/4080036f-afc5-422e-abe9-1c0c8e4f98ea/resource/3647e7b3-425e-4dcf-951b-cc4a22fb0129/download/europa_voyager_galileossi_global_mosaic_500m_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/europa.png"
  fetch_and_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/e1422336-3291-4b65-b903-c942d53de073/resource/eb32abd7-fee2-47d1-9f96-9d7d8824cc3a/download/ganymede_voyager_galileossi_global_clrmosaic_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/ganymede.png"
  fetch_and_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/a80abd68-7ed9-440e-829a-76376779164f/resource/ac628525-cb1c-4742-928b-5a0a60f372cd/download/callisto_voyager_galileossi_global_mosaic_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/callisto.png"
fi

# List what is actually on disk rather than what the script hoped to write —
# fast mode deliberately skips Vesta, so a fixed list would claim a file that
# may not be there.
echo "Science textures present:"
for name in ceres.png vesta.png pluto.png charon.png io.png europa.png ganymede.png callisto.png; do
  if [[ -f "${TEXTURE_DIR}/${name}" ]]; then
    echo "  ${TEXTURE_DIR}/${name}"
  else
    echo "  ${TEXTURE_DIR}/${name} (missing)"
  fi
done
