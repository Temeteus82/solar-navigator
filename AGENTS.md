# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Project Identity

Solar Navigator is an **AI agent coding experiment**. Every feature is co-created with AI
coding agents; the human author's role is to direct, review, and iterate — not to write code
manually. When making changes, favour clean, idiomatic Rust that a future agent can read and
extend with minimal context.

## Commands

### Build and run

```bash
# Portable build (no SPICE dependency — always works)
cargo run --release --no-default-features

# SPICE build (requires vendor/cspice set up first)
cargo run --release
```

### Quality checks (run both feature flag variants before committing)

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets

cargo check --all-targets --no-default-features
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-targets --no-default-features
```

### Run a single test

```bash
cargo test <test_name>
cargo test <test_name> --no-default-features
```

### Build cache and disk management

[sccache](https://github.com/mozilla/sccache) is configured as the compiler wrapper in
`.cargo/config.toml`. It caches compiled crate artefacts and shares them across all Rust
projects on the machine, cutting rebuild times significantly. Install it once:

```bash
brew install sccache          # macOS
cargo install sccache         # other platforms
```

[cargo-sweep](https://github.com/holmgr/cargo-sweep) prunes stale build artefacts without
a full `cargo clean`. Install once, then run periodically:

```bash
cargo install cargo-sweep

# Remove artefacts older than 7 days (default)
./scripts/sweep.sh

# Remove artefacts older than N days
./scripts/sweep.sh 14

# Stamp the current build as "in use" (run before a long break)
./scripts/sweep.sh stamp

# Nuke everything (like cargo clean)
./scripts/sweep.sh all
```

### First-time setup

```bash
# Linux — install system deps first (clang, libasound2-dev, libudev-dev,
# libwayland-dev, libx11-dev, libxcursor-dev, libxkbcommon-dev, libxi-dev,
# libxrandr-dev, pkg-config), then:
./scripts/setup_cspice_linux_x86_64.sh
./scripts/download_spice_kernels.sh
./scripts/download_textures_solar_system_scope.sh

# macOS (arm64)
./scripts/setup_cspice_macos_arm64.sh
./scripts/download_spice_kernels.sh
./scripts/download_textures_solar_system_scope.sh
```

```powershell
# Windows (x86_64) — native PowerShell, no bash required
./scripts/setup_cspice_windows_x86_64.ps1
./scripts/download_spice_kernels.ps1
./scripts/download_textures_solar_system_scope.ps1
./scripts/download_textures_minor_bodies_science.ps1
```

## Architecture

### Module layout

```
src/
  main.rs            — calls app::run()
  ephemeris.rs       — SpiceEphemeris + Horizons HTTP client (no Bevy)
  app/
    mod.rs           — Bevy App construction; registers all systems and resources
    types.rs         — every Resource, Component, and the BODIES constant array
    setup.rs         — Startup systems: scene spawn, texture loading, Horizons sync task
    simulation.rs    — Update systems: keyboard input, time advance, body positions
    camera.rs        — Update systems: orbit camera, jump/fly-to, tracking
    render.rs        — Update systems: lighting presets, visibility, window title
    materials.rs     — PlanetAtmosphereMaterial (custom Bevy Material backed by WGSL)
    ui.rs            — egui side panel (EguiPrimaryContextPass schedule)
    util.rs          — asset resolution, image/starfield helpers, format_simulation_speed
```

### Two build modes

The `spice` Cargo feature (on by default) gates all CSPICE integration with `#[cfg(feature = "spice")]`. Both code paths must compile and be correct:

- **SPICE mode**: loads NAIF kernels from `assets/spice/`, uses `rust-spice` for accurate ephemerides. Bodies not covered by the loaded kernels (Ceres, Vesta, Charon) silently fall back to analytic orbits.
- **Fallback/portable mode**: analytic Keplerian ellipses defined in `ephemeris.rs:orbit_for_target`. No native library needed.

`SpiceEphemeris` is stored as `NonSend` (`EphemerisResource`) because the SPICE lock (`Mutex<SpiceLock>`) must not be sent across threads.

### BODIES array and BodySpec

`types.rs:BODIES` is the canonical static array of all 14 rendered solar-system bodies. Every body's display name, SPICE target string, visual radius, texture filename, PBR parameters, spin rate, and atmosphere config live here. Body index is the stable identifier used everywhere (queries, positions vec, camera targeting).

### Coordinate remapping

SPICE uses the `ECLIPJ2000` frame (Z = ecliptic north, right-handed). Bevy uses Y-up. The remap in `simulation.rs:update_body_positions` is:

```
scene_x =  au_x  * scale
scene_y =  au_z  * scale   ← SPICE Z → Bevy Y
scene_z = -au_y  * scale   ← negate to preserve right-handedness
```

The same sign convention applies to the Horizons sync offset stored in `HorizonsSyncState::per_body_au_offset`.

### Horizons sync

On startup (SPICE mode only), `setup::start_horizons_sync` spawns an async task that calls NASA JPL Horizons for each body's current heliocentric position and computes per-body AU offsets relative to what SPICE reports (`per_body_au_offset`). These offsets are added each frame in `update_body_positions` to correct for any kernel/reference-frame drift. The task retries up to 5 times with exponential backoff (1 s, 2 s, 4 s … capped at 30 s). Manual retry is available via the UI button.

### Lighting and AU scale

The app uses a single Realistic lighting mode. `AU_TO_SCENE_UNITS = 250.0` is the fixed
scale constant in `types.rs`. `render.rs:apply_lighting_preset` configures the solar key
light (1.6 GW point light at origin, `shadows_enabled = true`), a faint directional sky
fill (5 lux), and a low ambient (0.3) so the Sun's inverse-square falloff creates a visible
brightness gradient from Mercury out to Neptune. Body visual radii are set to ~15× their
physical size so they are visible at solar-system scale without being artificially huge.

### Asset resolution order

At runtime `util::resolve_assets_root` checks in order:
1. `SOLAR_NAVIGATOR_ASSETS` environment variable
2. Paths relative to the executable (`./assets`, `../assets`, `../../assets`, `../share/solar-navigator/assets`)
3. macOS app bundle (`Contents/Resources/assets`)
4. Compile-time source-tree fallback (`CARGO_MANIFEST_DIR/assets`)

Textures and SPICE kernels are never bundled in the repo — download them with the scripts. Missing textures degrade gracefully to the body's fallback color.

### Custom atmosphere shader

`PlanetAtmosphereMaterial` (`materials.rs`) uses `assets/shaders/planet_atmosphere.wgsl`. It is rendered front-face-culled with additive blending and no depth write, creating a limb-glow halo. The `params` uniform encodes `(density, rim_power, forward_phase_power, brightness)`.

### CSPICE build wiring

`.cargo/config.toml` sets `CSPICE_DIR = vendor/cspice/cspice` and uses `scripts/linker-wrapper.sh` on macOS arm64. The setup scripts download and unpack the pre-built CSPICE toolkit into `vendor/cspice/`.

## Key conventions

- All internal `app/` types are `pub(super)` — nothing leaks out of the `app` module.
- Bevy resources are defined in `types.rs`; systems live in the module that owns them.
- Adding a new solar-system body requires only a new `BodySpec` entry in `BODIES` and, for SPICE, a mapping in `ephemeris.rs:horizons_command_for_target` and `spice_supports_target`.
- Both feature flag variants (`--features spice` and `--no-default-features`) must pass `cargo check`, `clippy -D warnings`, and `cargo test` cleanly — CI enforces this on every push.
