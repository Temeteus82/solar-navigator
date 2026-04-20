#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
cd "${PROJECT_ROOT}"

APP_NAME="Solar Navigator"
BUNDLE_ID="com.teemu.solarnavigator"
APP_VERSION="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
BUNDLE_ROOT="${PROJECT_ROOT}/dist/${APP_NAME}.app"
CONTENTS_DIR="${BUNDLE_ROOT}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
FAST_MODE="${FAST:-0}"

printf '\n[1/6] cargo fmt --check\n'
cargo fmt --check

printf '\n[2/6] cargo check\n'
cargo check

if [[ "${FAST_MODE}" != "1" ]]; then
  printf '\n[3/6] cargo clippy (deny warnings)\n'
  cargo clippy --all-targets -- -D warnings

  printf '\n[4/6] cargo test\n'
  cargo test
else
  printf '\n[3/6] FAST=1 -> skipping clippy\n'
  printf '\n[4/6] FAST=1 -> skipping tests\n'
fi

printf '\n[5/6] cargo build --release\n'
cargo build --release

if [[ ! -f "${PROJECT_ROOT}/assets/icon/AppIcon.icns" ]]; then
  printf '\nIcon not found, generating icon assets...\n'
  "${PROJECT_ROOT}/scripts/generate_app_icon.sh"
fi

printf '\n[6/6] Building app bundle\n'
rm -rf "${BUNDLE_ROOT}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cp "${PROJECT_ROOT}/target/release/solar-navigator" "${MACOS_DIR}/solar-navigator"
chmod +x "${MACOS_DIR}/solar-navigator"

cp "${PROJECT_ROOT}/assets/icon/AppIcon.icns" "${RESOURCES_DIR}/AppIcon.icns"
cp -R "${PROJECT_ROOT}/assets" "${RESOURCES_DIR}/assets"

cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleExecutable</key>
  <string>solar-navigator</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${APP_VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${APP_VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

plutil -lint "${CONTENTS_DIR}/Info.plist" >/dev/null

if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "${BUNDLE_ROOT}" >/dev/null
fi

printf '\nBundle ready:\n%s\n' "${BUNDLE_ROOT}"
