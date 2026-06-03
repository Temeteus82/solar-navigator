# GitHub Copilot Instructions — Solar Navigator

## Project Identity
Solar Navigator is an **AI agent coding experiment**: every feature is co-created with AI
coding agents. The human author directs and reviews; no manual code is written. Favour
clean, idiomatic Rust that future agents can extend with minimal context.

## Tech Stack
- Rust 2024 edition · Bevy 0.18.1 ECS · bevy_egui · wgpu v27 (Metal/Vulkan/DX12)
- `rust-spice` (optional `spice` Cargo feature) — NAIF CSPICE ephemerides
- `reqwest` — NASA JPL Horizons HTTP queries
- Custom WGSL atmosphere shader in `assets/shaders/planet_atmosphere.wgsl`

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
    camera.rs      — Update: orbit camera, fly-to animation, body tracking
    render.rs      — Update: solar lighting, visibility toggles, window title
    materials.rs   — PlanetAtmosphereMaterial (Bevy ExtendedMaterial + WGSL)
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
- Solar lighting: 1.6 GW point light at origin, `shadows_enabled = true`,
  ambient 0.3, sky fill 5 lux — inverse-square falloff dominates.

## Quality Gate (run before every commit)
```bash
cargo fmt --check
cargo check --all-targets && cargo check --all-targets --no-default-features
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-targets --no-default-features
```
