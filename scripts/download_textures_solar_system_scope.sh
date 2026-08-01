#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEXTURE_DIR="${ROOT_DIR}/assets/textures"
mkdir -p "${TEXTURE_DIR}"

BASE_URL="https://www.solarsystemscope.com/textures/download"

# Existing textures are left alone unless --force is passed, matching the
# PowerShell port's -Force. The shell script used to overwrite unconditionally,
# so simply following the documented setup steps on an existing checkout would
# replace the committed maps without asking.
FORCE=0
if [[ "${1:-}" == "--force" ]]; then
  FORCE=1
fi

# Solar System Scope answers an unknown texture name with HTTP 200 and an HTML
# error page rather than a 404 — 8k_uranus.jpg and 8k_neptune.jpg both do this.
# `curl -f` keys off the status code, so it never fires and the HTML gets
# written into a .jpg. Check the bytes that arrived, not the status line.
fetch_texture() {
  local remote="$1"
  local name="$2"
  local dest="${TEXTURE_DIR}/${name}"

  if [[ -f "$dest" && "$FORCE" != "1" ]]; then
    echo "Skipping ${name} (already present, use --force to re-download)"
    return 0
  fi

  local tmp
  tmp="$(mktemp "${TMPDIR:-/tmp}/sss-texture-XXXXXX")"
  curl -fL --retry 3 --retry-delay 1 "${BASE_URL}/${remote}" -o "$tmp"

  local mime
  mime="$(file -b --mime-type "$tmp")"
  if [[ "$mime" != image/* ]]; then
    rm -f "$tmp"
    echo "Refusing to write ${name}: ${remote} returned ${mime}, not an image" >&2
    return 1
  fi

  mv "$tmp" "$dest"
  chmod 0644 "$dest"
  echo "Downloaded ${name} (${remote})"
}

# The resolutions below reproduce the maps committed to assets/textures. This
# list previously asked for the "2k_" variants of eight bodies that are
# committed at 4K or 8K, so running the documented setup downgraded them by up
# to 16x in linear resolution.
#
# Solar System Scope's "8k_" prefix is a product name, not a guarantee: 8k_sun,
# 8k_jupiter and 8k_saturn are all served at 4096x2048. Uranus and Neptune have
# no 8k product at all (those names soft-404 into HTML), so they stay on 2k_,
# which is what their committed maps already are.
fetch_texture 8k_sun.jpg              sun.jpg          # 4096x2048
fetch_texture 8k_mercury.jpg          mercury.jpg      # 8192x4096
fetch_texture 8k_venus_surface.jpg    venus.jpg        # 8192x4096
fetch_texture 8k_earth_daymap.jpg     earth.jpg        # 8192x4096
fetch_texture 8k_moon.jpg             moon.jpg         # 8192x4096
fetch_texture 8k_mars.jpg             mars.jpg         # 8192x4096
fetch_texture 8k_jupiter.jpg          jupiter.jpg      # 4096x2048
fetch_texture 8k_saturn.jpg           saturn.jpg       # 4096x2048
fetch_texture 8k_saturn_ring_alpha.png saturn_ring.png # 8192x500
fetch_texture 2k_uranus.jpg           uranus.jpg       # 2048x1024 (no 8k product)
fetch_texture 2k_neptune.jpg          neptune.jpg      # 2048x1024 (no 8k product)
fetch_texture 8k_stars_milky_way.jpg  milky_way_8k.jpg # 8192x4096

echo "Planet textures downloaded to ${TEXTURE_DIR}"
echo "Milky Way texture downloaded to ${TEXTURE_DIR}/milky_way_8k.jpg"
echo "Reminder: verify current license/attribution requirements before redistribution."
