#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEXTURE_DIR="${ROOT_DIR}/assets/textures"
mkdir -p "${TEXTURE_DIR}"

# FULL_RES=1 downloads heavy science products (hundreds of MB each) and
# converts them to JPEG for runtime usage.
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

convert_to_jpeg() {
  local src="$1"
  local dest="$2"
  # Use sips so the script works on stock macOS.
  sips --resampleWidth "${TARGET_WIDTH}" --setProperty format jpeg "$src" --out "$dest" >/dev/null
}

copy_or_convert() {
  local url="$1"
  local src_ext="$2"
  local dest="$3"

  if [[ -f "$dest" && "$FORCE" != "1" ]]; then
    echo "Skipping $(basename "$dest") (already present, use --force to re-download)"
    return 0
  fi

  local tmp
  tmp="$(fetch_to_tmp "$url" "$src_ext")"
  trap 'rm -rf "$(dirname "$tmp")"' RETURN

  if [[ "$src_ext" == "jpg" || "$src_ext" == "jpeg" ]]; then
    cp "$tmp" "$dest"
  else
    convert_to_jpeg "$tmp" "$dest"
  fi

  chmod 0644 "$dest"
}

require_cmd curl
require_cmd sips

if [[ "$FULL_RES" == "1" ]]; then
  echo "Downloading FULL-RES science mosaics (large files)..."
  copy_or_convert \
    "https://dawngis.dlr.de/data/Ceres/mosaics/HAMO/clear/Ceres_HAMO_mosaic_global.png" \
    "png" \
    "${TEXTURE_DIR}/ceres.jpg"
  copy_or_convert \
    "https://dawngis.dlr.de/data/Vesta/mosaics/HAMO/truecolor/Vesta_true_color_HAMO-1-2_global.png" \
    "png" \
    "${TEXTURE_DIR}/vesta.jpg"
  copy_or_convert \
    "https://planetarymaps.usgs.gov/mosaic/Pluto_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif" \
    "tif" \
    "${TEXTURE_DIR}/pluto.jpg"
  copy_or_convert \
    "https://planetarymaps.usgs.gov/mosaic/Charon_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif" \
    "tif" \
    "${TEXTURE_DIR}/charon.jpg"
  # Galilean moons, USGS Astrogeology global mosaics. Io and Ganymede have
  # published colour-merge products; Europa and Callisto exist only as
  # greyscale mosaics there, so those two render monochrome (no colour
  # equirectangular map of them is published by USGS).
  copy_or_convert \
    "https://planetarymaps.usgs.gov/mosaic/Io_Galileo_SSI_Global_Mosaic_ClrMerge_1km.tif" \
    "tif" \
    "${TEXTURE_DIR}/io.jpg"
  copy_or_convert \
    "https://planetarymaps.usgs.gov/mosaic/Europa_Voyager_GalileoSSI_global_mosaic_500m.tif" \
    "tif" \
    "${TEXTURE_DIR}/europa.jpg"
  copy_or_convert \
    "https://planetarymaps.usgs.gov/mosaic/Ganymede_Voyager_GalileoSSI_Global_ClrMosaic_1435m.tif" \
    "tif" \
    "${TEXTURE_DIR}/ganymede.jpg"
  copy_or_convert \
    "https://planetarymaps.usgs.gov/mosaic/Callisto_Voyager_GalileoSSI_global_mosaic_1km.tif" \
    "tif" \
    "${TEXTURE_DIR}/callisto.jpg"
else
  echo "Downloading compact science textures (fast mode)..."
  copy_or_convert \
    "https://dawngis.dlr.de/data/Ceres/mosaics/HAMO/clear/Ceres_HAMO_mosaic_preview.png" \
    "png" \
    "${TEXTURE_DIR}/ceres.jpg"
  copy_or_convert \
    "https://dawngis.dlr.de/data/Vesta/mosaics/HAMO/truecolor/Vesta_true_color_HAMO-1-2.png" \
    "png" \
    "${TEXTURE_DIR}/vesta.jpg"
  copy_or_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/a5f1b7f4-9822-4697-a201-e23ef4bd3e16/resource/96be2aa1-f384-4a9f-9458-a8431a0e7956/download/pluto_newhorizons_global_mosaic_300m_jul2017_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/pluto.jpg"
  copy_or_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/93827f6c-8feb-42b6-98e6-b0ce57c7d2c8/resource/1abf318c-3290-4aa0-932e-a34f32d7f6ad/download/charon_newhorizons_global_mosaic_300m_jul2017_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/charon.jpg"
  # 1024-wide browse renderings of the same USGS mosaics used above.
  copy_or_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/0fc15885-24ee-4d9d-9666-11de0667c10c/resource/73d4c1f7-8c07-4b28-90ea-f47f7531c5ca/download/full.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/io.jpg"
  copy_or_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/4080036f-afc5-422e-abe9-1c0c8e4f98ea/resource/3647e7b3-425e-4dcf-951b-cc4a22fb0129/download/europa_voyager_galileossi_global_mosaic_500m_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/europa.jpg"
  copy_or_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/e1422336-3291-4b65-b903-c942d53de073/resource/eb32abd7-fee2-47d1-9f96-9d7d8824cc3a/download/ganymede_voyager_galileossi_global_clrmosaic_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/ganymede.jpg"
  copy_or_convert \
    "https://astrogeology.usgs.gov/ckan/dataset/a80abd68-7ed9-440e-829a-76376779164f/resource/ac628525-cb1c-4742-928b-5a0a60f372cd/download/callisto_voyager_galileossi_global_mosaic_1024.jpg" \
    "jpg" \
    "${TEXTURE_DIR}/callisto.jpg"
fi

echo "Science textures saved:"
echo "  ${TEXTURE_DIR}/ceres.jpg"
echo "  ${TEXTURE_DIR}/vesta.jpg"
echo "  ${TEXTURE_DIR}/pluto.jpg"
echo "  ${TEXTURE_DIR}/charon.jpg"
echo "  ${TEXTURE_DIR}/io.jpg"
echo "  ${TEXTURE_DIR}/europa.jpg"
echo "  ${TEXTURE_DIR}/ganymede.jpg"
echo "  ${TEXTURE_DIR}/callisto.jpg"
