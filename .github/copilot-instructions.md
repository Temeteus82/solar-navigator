# GitHub Copilot Instructions — Solar Navigator

## Project Identity
Solar Navigator is an **AI agent coding experiment**: every feature is co-created with AI
coding agents. The human author directs and reviews; no manual code is written. Favour
clean, idiomatic Rust that future agents can extend with minimal context.

## Tech Stack
- Rust 2024 edition · Bevy 0.19 ECS · bevy_egui · wgpu v29 (Metal/Vulkan/DX12)
- `rust-spice` (optional `spice` Cargo feature) — NAIF CSPICE ephemerides
- `reqwest` — NASA JPL Horizons HTTP queries
- Custom WGSL shaders: atmosphere halo (`assets/shaders/planet_atmosphere.wgsl`) and
  Saturn's ring shadow (`assets/shaders/planet_ring.wgsl`)

## Architecture
```
src/
  main.rs          — entry point, calls app::run()
  ephemeris.rs     — SpiceEphemeris + Horizons client (pure Rust, no Bevy)
  app/
    mod.rs         — Bevy App construction and system registration
    types.rs       — Resources, Components, BODIES array (18 solar-system bodies)
    setup.rs       — Startup: scene spawn, textures, Horizons sync task
    simulation.rs  — Update: keyboard, time advance, body positions, spin
    camera.rs      — Update: orbit + free-fly cameras (F toggles), fly-to animation, body tracking
    render.rs      — Update: solar lighting, visibility toggles, window title
    asteroids.rs   — Update: procedural asteroid belt (Keplerian swarm)
    materials.rs   — PlanetAtmosphereMaterial + PlanetRingMaterial (custom Bevy Material + WGSL)
    ui.rs          — egui side panel (target search, sim controls, body list)
    util.rs        — asset resolution, cubemap conversion, format helpers
```

## Rules
- `pub(super)` everywhere inside `app/` — nothing leaks out of the module boundary.
- Resources defined in `types.rs`; systems live in the module that owns them.
- Both `--features spice` and `--no-default-features` must pass `cargo check`,
  `cargo clippy -- -D warnings`, and `cargo test` at all times.
- `BODIES` in `types.rs` is the single source of truth for all rendered bodies.
- Coordinate remap: SPICE Z → Bevy Y; negate SPICE Y → Bevy Z (right-handed).
- AU scale: `AU_TO_SCENE_UNITS = 250.0` (constant, no runtime switching).
- Body visual radii are ~15× physical size so they are visible at solar-system scale.
- Solar lighting: DirectionalLight `sky_fill` (1800 lux, shadows on) re-aimed each frame
  from the Sun toward the camera focus so every body is evenly lit; PointLight `solar_key`
  dimmed to 80 MW with shadows off (specular/bloom near the Sun only); ambient 0.25.
  `MainCamera` post-processing: AutoExposure, Bloom, SSAO.
- Camera modes: `F` toggles Orbit (tethered, click-to-fly-to) vs Free (WASD/QE fly-cam,
  drag to look, Shift to boost); hand-off is seamless in both directions.

## Quality Gate (run before every commit)
```bash
cargo fmt --check
cargo check --all-targets && cargo check --all-targets --no-default-features
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-targets --no-default-features
```
