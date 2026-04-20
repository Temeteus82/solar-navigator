# Solar Navigator (Rust + SPICE/Fallback)

Cross-platform desktop app for a 3D solar-system navigator (macOS, Linux, Windows):

- 3D scene with interactive orbit camera (`bevy` + `bevy_egui`)
- SPICE-based planet/moon ephemerides (`rust-spice` + NAIF kernels)
- Side-panel target search with smooth camera fly-to
- Texture-driven rendering with graceful fallback when textures are missing
- PBR material tuning, emissive sun, atmosphere halos, and starfield backdrop / Milky Way sky
- Analytic fallback orbits when kernels are not present yet

## Quick Start (Portable Build)

```bash
cd /path/to/solar-navigator
./scripts/download_spice_kernels.sh
./scripts/download_textures_solar_system_scope.sh
./scripts/download_textures_minor_bodies_science.sh
cargo run --release --no-default-features
```

If you are already in the project directory, skip the `cd` step.

## Quick Start (macOS SPICE Build)

```bash
cd /path/to/solar-navigator
./scripts/setup_cspice_macos_arm64.sh
./scripts/download_spice_kernels.sh
./scripts/download_textures_solar_system_scope.sh
./scripts/download_textures_minor_bodies_science.sh
./scripts/generate_app_icon.sh
cargo run --release
```

The default Cargo feature set enables SPICE (`--features spice`).

## Quick Start (Linux x86_64 SPICE Build)

```bash
cd /path/to/solar-navigator
./scripts/setup_cspice_linux_x86_64.sh
./scripts/download_spice_kernels.sh
./scripts/download_textures_solar_system_scope.sh
./scripts/download_textures_minor_bodies_science.sh
cargo run --release --features spice
```

## Quick Start (Windows x86_64 SPICE Build)

```powershell
Set-Location C:\path\to\solar-navigator
.\scripts\setup_cspice_windows_x86_64.ps1
bash ./scripts/download_spice_kernels.sh
bash ./scripts/download_textures_solar_system_scope.sh
bash ./scripts/download_textures_minor_bodies_science.sh
cargo run --release --features spice
```

The texture and kernel download scripts are Bash scripts; run them from Git Bash/WSL, or invoke them from PowerShell via `bash ...` as shown above.

## Controls

- Right mouse drag + wheel: orbit/zoom camera
- Left side panel: search targets, jump directly, and switch lighting presets
- `Space`: pause/unpause simulation
- `Up`: speed up simulation time
- `Down`: slow down simulation time
- `Backspace`: reset simulation state and camera target

## Modes

- **SPICE mode**: used when required kernels exist under `assets/spice`.
- **Fallback mode**: used automatically when kernels are absent, or when the app is built with `--no-default-features`.

The window title and overlay show current mode and selection.

## Quality Checks

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets

# Also validate the portable (non-SPICE) build profile:
cargo check --all-targets --no-default-features
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-targets --no-default-features
```

## Asset Resolution

At runtime, assets are resolved in this order:

1. `SOLAR_NAVIGATOR_ASSETS` environment variable (if it points to a directory)
2. Path relative to executable (`./assets`, `../assets`, `../../assets`, `../share/solar-navigator/assets`)
3. macOS app bundle path (`Solar Navigator.app/Contents/Resources/assets`)
4. Source-tree fallback (`<repo>/assets`)

## Bundle Build (macOS)

```bash
cd /path/to/solar-navigator
./scripts/package_macos_arm64.sh
```

Produces: `dist/Solar Navigator.app` (macOS only).

## Packaging (Linux)

```bash
cd /path/to/solar-navigator
./scripts/package_linux.sh
```

Outputs are written to `dist/linux`:
- `.tar.gz` portable archive (always)
- `.deb` package (if `dpkg-deb` is installed)
- `.AppImage` (if `appimagetool` is installed)

Use `WITH_SPICE=1 ./scripts/package_linux.sh` to package a SPICE-enabled build.

## Linux Distro Coverage

CI validates native builds on major Linux distributions:
- Ubuntu 24.04
- Debian 12
- Fedora 42
- Arch Linux (latest)

For each Linux distro above, CI runs both:
- portable mode (`--no-default-features`)
- SPICE mode (`--no-default-features --features spice`)

Windows CI also validates both portable and SPICE modes on native `windows-latest`.

## Packaging (Windows)

```powershell
Set-Location C:\path\to\solar-navigator
.\scripts\package_windows.ps1
```

Outputs are written to `dist/windows`:
- `.zip` portable package (always)
- `.msi` installer (if WiX `wix` CLI is installed)

Use `.\scripts\package_windows.ps1 -WithSpice` for a SPICE-enabled package.

## Asset Notes

- `scripts/setup_cspice_macos_arm64.sh` installs an arm64 CSPICE toolkit under `vendor/cspice` and is required for native Apple Silicon SPICE linking.
- `scripts/setup_cspice_linux_x86_64.sh` installs Linux x86_64 CSPICE under `vendor/cspice`.
- `scripts/setup_cspice_windows_x86_64.ps1` installs Windows x86_64 CSPICE under `vendor/cspice`.
- Linux CI validates both portable and SPICE modes across Ubuntu, Debian, Fedora, and Arch.
- Windows CI validates portable mode and SPICE mode on native windows-latest.
- `scripts/generate_app_icon.sh` creates `assets/icon/AppIcon.icns` and `assets/icon/AppIcon.iconset/` for macOS app packaging.
- SPICE kernels are downloaded from NAIF/JPL public servers.
- Planet textures in the provided script come from Solar System Scope. Check and follow their latest terms/attribution requirements before redistribution.

Texture filenames expected by the app are documented in `assets/textures/README.md`.

## Known Issues

- **Moon/Earth visual scale mismatch (open bug):** Even with physically scaled center-to-center distances, the current body render sizes can still make the Moon appear unrealistically close to Earth in some camera/preset combinations. A separate visual-scaling model (independent from orbital distance scale) is still needed.

## Project Policies

- License: `LICENSE` (MIT)
- Third-party notices: `THIRD_PARTY_NOTICES.md`
- Asset attribution: `ASSET_ATTRIBUTION.md`
- Contributing guide: `CONTRIBUTING.md`
- Security policy: `.github/SECURITY.md`
