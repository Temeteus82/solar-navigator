#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This packaging script targets Linux hosts only."
  exit 1
fi

APP_SLUG="solar-navigator"
APP_DISPLAY_NAME="Solar Navigator"
VERSION="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
DIST_ROOT="${PROJECT_ROOT}/dist/linux"
PACKAGE_ROOT="${DIST_ROOT}/${APP_SLUG}-${VERSION}-linux-x86_64"
DEB_ROOT="${DIST_ROOT}/deb-root"
APPDIR_ROOT="${DIST_ROOT}/AppDir"
BINARY_SRC="${PROJECT_ROOT}/target/release/${APP_SLUG}"
ICON_SRC="${PROJECT_ROOT}/assets/icon/AppIcon.iconset/icon_512x512.png"

mkdir -p "${DIST_ROOT}"

build_args=(--release)
if [[ "${WITH_SPICE:-0}" != "1" ]]; then
  build_args+=(--no-default-features)
fi

echo "[1/4] Building release binary (cargo build ${build_args[*]})"
cargo build "${build_args[@]}"

if [[ ! -x "${BINARY_SRC}" ]]; then
  echo "Build did not produce ${BINARY_SRC}"
  exit 1
fi

echo "[2/4] Creating portable tar.gz package"
rm -rf "${PACKAGE_ROOT}"
mkdir -p "${PACKAGE_ROOT}/bin" "${PACKAGE_ROOT}/share/${APP_SLUG}"
cp "${BINARY_SRC}" "${PACKAGE_ROOT}/bin/${APP_SLUG}"
cp -R "${PROJECT_ROOT}/assets" "${PACKAGE_ROOT}/share/${APP_SLUG}/assets"

tar -czf "${DIST_ROOT}/${APP_SLUG}-${VERSION}-linux-x86_64.tar.gz" \
  -C "${DIST_ROOT}" "$(basename "${PACKAGE_ROOT}")"

echo "[3/4] Creating .deb package (if dpkg-deb is available)"
if command -v dpkg-deb >/dev/null 2>&1; then
  rm -rf "${DEB_ROOT}"
  mkdir -p \
    "${DEB_ROOT}/DEBIAN" \
    "${DEB_ROOT}/usr/bin" \
    "${DEB_ROOT}/usr/share/${APP_SLUG}" \
    "${DEB_ROOT}/usr/share/applications" \
    "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps"

  cp "${BINARY_SRC}" "${DEB_ROOT}/usr/bin/${APP_SLUG}"
  cp -R "${PROJECT_ROOT}/assets" "${DEB_ROOT}/usr/share/${APP_SLUG}/assets"

  if [[ -f "${ICON_SRC}" ]]; then
    cp "${ICON_SRC}" "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps/${APP_SLUG}.png"
  fi

  cat > "${DEB_ROOT}/usr/share/applications/${APP_SLUG}.desktop" <<DESKTOP
[Desktop Entry]
Name=${APP_DISPLAY_NAME}
Comment=3D solar-system navigator
Exec=${APP_SLUG}
Icon=${APP_SLUG}
Terminal=false
Type=Application
Categories=Science;Education;
DESKTOP

  cat > "${DEB_ROOT}/DEBIAN/control" <<CONTROL
Package: ${APP_SLUG}
Version: ${VERSION}
Section: science
Priority: optional
Architecture: amd64
Maintainer: Solar Navigator Contributors
Depends: libasound2, libudev1, libx11-6, libxrandr2, libxi6, libxcursor1, libxinerama1, libxkbcommon0, libwayland-client0
Description: ${APP_DISPLAY_NAME}
 3D solar-system navigator built with Rust and Bevy.
CONTROL

  dpkg-deb --build "${DEB_ROOT}" "${DIST_ROOT}/${APP_SLUG}_${VERSION}_amd64.deb"
else
  echo "  Skipping .deb build: dpkg-deb was not found."
fi

echo "[4/4] Creating AppImage (if appimagetool is available)"
if command -v appimagetool >/dev/null 2>&1; then
  rm -rf "${APPDIR_ROOT}"
  mkdir -p \
    "${APPDIR_ROOT}/usr/bin" \
    "${APPDIR_ROOT}/usr/share/${APP_SLUG}"

  cp "${BINARY_SRC}" "${APPDIR_ROOT}/usr/bin/${APP_SLUG}"
  cp -R "${PROJECT_ROOT}/assets" "${APPDIR_ROOT}/usr/share/${APP_SLUG}/assets"

  cat > "${APPDIR_ROOT}/AppRun" <<'APPRUN'
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export SOLAR_NAVIGATOR_ASSETS="${SCRIPT_DIR}/usr/share/solar-navigator/assets"
exec "${SCRIPT_DIR}/usr/bin/solar-navigator" "$@"
APPRUN
  chmod +x "${APPDIR_ROOT}/AppRun"

  cat > "${APPDIR_ROOT}/${APP_SLUG}.desktop" <<DESKTOP
[Desktop Entry]
Name=${APP_DISPLAY_NAME}
Comment=3D solar-system navigator
Exec=${APP_SLUG}
Icon=${APP_SLUG}
Terminal=false
Type=Application
Categories=Science;Education;
DESKTOP

  if [[ -f "${ICON_SRC}" ]]; then
    cp "${ICON_SRC}" "${APPDIR_ROOT}/${APP_SLUG}.png"
  fi

  ARCH=x86_64 appimagetool "${APPDIR_ROOT}" "${DIST_ROOT}/${APP_SLUG}-${VERSION}-x86_64.AppImage"
else
  echo "  Skipping AppImage build: appimagetool was not found."
fi

echo "Linux packages available in ${DIST_ROOT}"
