#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEXTURE_DIR="${ROOT_DIR}/assets/textures"
mkdir -p "${TEXTURE_DIR}"

# FULL_RES=1 downloads heavy science products (hundreds of MB each) and
# converts them to JPEG for runtime usage.
FULL_RES="${FULL_RES:-0}"
TARGET_WIDTH="${TARGET_WIDTH:-4096}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

fetch_to_tmp() {
  local url="$1"
  local suffix="$2"
  local tmp
  tmp="$(mktemp "${TMPDIR:-/tmp}/minor-body-XXXXXX.${suffix}")"
  curl -fL --retry 3 --retry-delay 1 "$url" -o "$tmp"
  printf "%s" "$tmp"
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

  local tmp
  tmp="$(fetch_to_tmp "$url" "$src_ext")"
  trap 'rm -f "$tmp"' RETURN

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
fi

echo "Minor-body textures saved:"
echo "  ${TEXTURE_DIR}/ceres.jpg"
echo "  ${TEXTURE_DIR}/vesta.jpg"
echo "  ${TEXTURE_DIR}/pluto.jpg"
echo "  ${TEXTURE_DIR}/charon.jpg"
