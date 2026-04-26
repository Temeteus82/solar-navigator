#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEXTURE_DIR="${ROOT_DIR}/assets/textures"
mkdir -p "${TEXTURE_DIR}"

BASE_URL="https://www.solarsystemscope.com/textures/download"

# Solar System Scope's 8k_sun is currently 4096x2048 and gives better detail
# than the previous 2k map used for the app's emissive sun.
curl -fL "${BASE_URL}/8k_sun.jpg" -o "${TEXTURE_DIR}/sun.jpg"
curl -fL "${BASE_URL}/2k_mercury.jpg" -o "${TEXTURE_DIR}/mercury.jpg"
curl -fL "${BASE_URL}/2k_venus_surface.jpg" -o "${TEXTURE_DIR}/venus.jpg"
curl -fL "${BASE_URL}/2k_earth_daymap.jpg" -o "${TEXTURE_DIR}/earth.jpg"
curl -fL "${BASE_URL}/2k_moon.jpg" -o "${TEXTURE_DIR}/moon.jpg"
curl -fL "${BASE_URL}/2k_mars.jpg" -o "${TEXTURE_DIR}/mars.jpg"
curl -fL "${BASE_URL}/2k_jupiter.jpg" -o "${TEXTURE_DIR}/jupiter.jpg"
curl -fL "${BASE_URL}/2k_saturn.jpg" -o "${TEXTURE_DIR}/saturn.jpg"
curl -fL "${BASE_URL}/2k_saturn_ring_alpha.png" -o "${TEXTURE_DIR}/saturn_ring.png"
curl -fL "${BASE_URL}/2k_uranus.jpg" -o "${TEXTURE_DIR}/uranus.jpg"
curl -fL "${BASE_URL}/2k_neptune.jpg" -o "${TEXTURE_DIR}/neptune.jpg"
curl -fL "${BASE_URL}/8k_stars_milky_way.jpg" -o "${TEXTURE_DIR}/milky_way_8k.jpg"

echo "Planet textures downloaded to ${TEXTURE_DIR}"
echo "Milky Way texture downloaded to ${TEXTURE_DIR}/milky_way_8k.jpg"
echo "Reminder: verify current license/attribution requirements before redistribution."
