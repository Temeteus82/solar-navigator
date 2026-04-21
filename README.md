# Solar Navigator — An AI Agent Coding Experiment

> **This project exists to test and demonstrate the capabilities of AI coding agents.**
> Every line of code, every architectural decision, and every feature in this repository
> has been written collaboratively with AI agents — no manual coding by the human author.
> The goal is to discover how far a non-trivial, real-world Rust application can be taken
> when the developer's role is purely to direct, review, and iterate with AI tools.

This is a 3D solar-system navigator written in Rust + Bevy. It is both a genuinely useful
application and a live benchmark of what AI agents can build, maintain, and improve in a
complex systems-programming project.

---

## Application Features

- 3D scene with interactive orbit camera (`bevy` + `bevy_egui`)
- SPICE-based planet/moon ephemerides (`rust-spice` + NAIF kernels)
- Side-panel target search with smooth camera fly-to
- Texture-driven PBR rendering with atmosphere halos and Milky Way starfield
- Realistic solar lighting with inverse-square falloff across the solar system
- Point-light shadow maps and screen-space ambient occlusion (SSAO)
- Analytic fallback orbits when SPICE kernels are absent

---

## Quick Start (Portable Build)

```bash
cd /path/to/solar-navigator
./scripts/download_spice_kernels.sh
./scripts/download_textures_solar_system_scope.sh
./scripts/download_textures_minor_bodies_science.sh
cargo run --release --no-default-features
```

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

The texture and kernel download scripts are Bash scripts; run them from Git Bash/WSL, or
invoke them from PowerShell via `bash ...` as shown above.

---

## Controls

| Input | Action |
|-------|--------|
| Right drag / scroll | Orbit and zoom camera |
| Shift + left drag | Pan camera |
| `Space` | Pause / unpause simulation |
| `Up` / `Down` | Increase / decrease simulation speed |
| `Backspace` | Reset simulation state and camera |
| Side panel | Search targets, select body, jump camera |

## Modes

- **SPICE mode** — used when required kernels exist under `assets/spice/`.
- **Fallback mode** — analytic Keplerian orbits; used automatically when kernels are absent
  or when built with `--no-default-features`.

The window title shows the current mode and selected body.

---

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

---

## Asset Resolution

At runtime, assets are resolved in this order:

1. `SOLAR_NAVIGATOR_ASSETS` environment variable (if it points to a directory)
2. Path relative to executable (`./assets`, `../assets`, `../../assets`, `../share/solar-navigator/assets`)
3. macOS app bundle path (`Solar Navigator.app/Contents/Resources/assets`)
4. Source-tree fallback (`<repo>/assets`)

---

## Bundle / Package Builds

```bash
# macOS
./scripts/package_macos_arm64.sh           # → dist/Solar Navigator.app

# Linux
./scripts/package_linux.sh                 # → dist/linux/ (.tar.gz, .deb, .AppImage)
WITH_SPICE=1 ./scripts/package_linux.sh    # SPICE-enabled build

# Windows (PowerShell)
.\scripts\package_windows.ps1              # → dist/windows/ (.zip, .msi)
.\scripts\package_windows.ps1 -WithSpice
```

## Linux Distro Coverage

CI validates native builds on:

- Ubuntu 24.04 · Debian 12 · Fedora 42 · Arch Linux

Each distro is tested in both portable (`--no-default-features`) and SPICE modes.

---

## Asset Notes

- Textures from Solar System Scope — check their terms before redistributing.
- SPICE kernels downloaded from NAIF/JPL public servers.
- Texture filenames expected by the app are documented in `assets/textures/README.md`.
- `scripts/generate_app_icon.sh` creates `assets/icon/AppIcon.icns` for macOS packaging.

---

## Project Policies

| Document | Purpose |
|----------|---------|
| `LICENSE` | MIT |
| `THIRD_PARTY_NOTICES.md` | Third-party dependency notices |
| `ASSET_ATTRIBUTION.md` | Texture / asset attributions |
| `CONTRIBUTING.md` | Contribution guide |
| `.github/SECURITY.md` | Security policy |
