#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEXTURE_DIR="${ROOT_DIR}/assets/textures"
mkdir -p "${TEXTURE_DIR}"

BASE_URL="https://www.solarsystemscope.com/textures/download"

# Existing textures are left alone unless --force is passed, matching the
# PowerShell port's -Force. The shell script used to overwrite unconditionally,
# so simply following the documented setup steps on an existing checkout would
# re-download every map without asking.
FORCE=0
if [[ "${1:-}" == "--force" ]]; then
  FORCE=1
fi

# ImageMagick covers every supported platform; sips stays last so a stock macOS
# box still works with nothing installed. Mirrors the minor-bodies script.
# `convert` is also the name of Windows' filesystem conversion tool, which sits
# in PATH under Git Bash and answers -version with "Invalid drive
# specification". Probe for the ImageMagick banner so that impostor is never
# selected — otherwise every re-encode fails after the download has already
# succeeded.
check_candidate() {
  command -v "$1" >/dev/null 2>&1 || return 1
  [[ "$1" != "convert" ]] || "$1" -version 2>&1 | grep -qi imagemagick
}

CONVERTER=""
for candidate in magick convert sips; do
  if check_candidate "$candidate"; then
    CONVERTER="$candidate"
    break
  fi
done
if [[ -z "$CONVERTER" ]]; then
  echo "Missing an image converter: install ImageMagick (magick/convert), or run on macOS where sips is built in." >&2
  exit 1
fi

# The upstream products are JPEG but every destination is .png, so downloads are
# re-encoded rather than moved — a .png must never end up holding JPEG bytes.
# PNG32:/format png pins 8-bit RGBA, the pixel format compress_textures.* feeds
# the BC7/ASTC encoders.
convert_to_png() {
  local src="$1"
  local dest="$2"
  case "$CONVERTER" in
    magick | convert) "$CONVERTER" "$src" -colorspace sRGB "PNG32:${dest}" ;;
    sips) sips --setProperty format png "$src" --out "$dest" >/dev/null ;;
  esac
}

# Solar System Scope answers an unknown texture name with HTTP 200 and an HTML
# error page rather than a 404 — 8k_uranus.jpg and 8k_neptune.jpg both do this.
# `curl -f` keys off the status code, so it never fires and the HTML gets
# written into the texture file. Check the bytes that arrived, not the status
# line.
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

  if [[ "${remote##*.}" == "png" ]]; then
    mv "$tmp" "$dest"
  else
    convert_to_png "$tmp" "$dest"
    rm -f "$tmp"
  fi
  chmod 0644 "$dest"
  echo "Downloaded ${name} (${remote})"
}

# These textures are the app's only source for these bodies — nothing under
# assets/textures/ is stored in git — so each entry asks for the largest product
# that body actually has. An earlier version of this list asked for the "2k_"
# variants of eight bodies published at 4K or 8K, which downgraded them by up to
# 16x in linear resolution.
#
# Solar System Scope's "8k_" prefix is a product name, not a guarantee: 8k_sun,
# 8k_jupiter and 8k_saturn are all served at 4096x2048. Uranus, Neptune and the
# Venus cloud layer have no 8k product at all (those names soft-404 into HTML),
# so they stay at the largest size that exists.
fetch_texture 8k_sun.jpg              sun.png          # 4096x2048
fetch_texture 8k_mercury.jpg          mercury.png      # 8192x4096
fetch_texture 8k_venus_surface.jpg    venus.png        # 8192x4096
fetch_texture 4k_venus_atmosphere.jpg venus_clouds.png # 4096x2048 (no 8k product)
fetch_texture 8k_earth_daymap.jpg     earth.png        # 8192x4096
fetch_texture 8k_moon.jpg             moon.png         # 8192x4096
fetch_texture 8k_mars.jpg             mars.png         # 8192x4096
fetch_texture 8k_jupiter.jpg          jupiter.png      # 4096x2048
fetch_texture 8k_saturn.jpg           saturn.png       # 4096x2048
fetch_texture 8k_saturn_ring_alpha.png saturn_ring.png # 8192x500
fetch_texture 2k_uranus.jpg           uranus.png       # 2048x1024 (no 8k product)
fetch_texture 2k_neptune.jpg          neptune.png      # 2048x1024 (no 8k product)
fetch_texture 8k_stars_milky_way.jpg  milky_way_8k.png # 8192x4096

echo "Planet textures downloaded to ${TEXTURE_DIR}"
echo "Milky Way texture downloaded to ${TEXTURE_DIR}/milky_way_8k.png"
echo "Reminder: verify current license/attribution requirements before redistribution."
