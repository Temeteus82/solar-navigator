# Full-repository code review, 2026-10-09

Static review of every Rust source file, both WGSL shaders, the CI and Claude
workflows and the script headers, at `afb0fd8` (merge of PR #90). Three claims
were verified against primary sources: the `de440s.bsp` comment area, and the
rust-spice 1.0.1, bevy_mesh 0.19.1 and bevy_post_process 0.19.1 crate sources.
The test suite was not run as part of the review.

Status key: **fixed** (in the PR that added this file), **open**.

## Must fix

1. **SPICE mode aborts the process on a date outside the kernel's coverage.** —
   **fixed**
   The date picker allows 1600–2200 (`src/app/ui.rs`), but `de440s.bsp` covers
   only 1849-12-26 to 2150-01-22. The next frame calls `spkpos` for every planet
   (`src/ephemeris.rs`). rust-spice never changes CSPICE's error action and the
   app never called `quiet()` or `check()`, so CSPICE stayed on its default
   ABORT: it printed a traceback and exited. Portable mode was unaffected.
   Fix: `quiet()` after acquiring the lock, `check()` after every SPICE call
   with a fall back to the analytic orbits, read the coverage with `spkcov` and
   clamp the date picker to it in SPICE mode.

## Should fix

2. **Body-to-layer syncs are unordered relative to the ephemeris pass.** — open
   The first `Update` tuple in `src/app/mod.rs` is not chained.
   `update_body_positions` is a NonSend system pinned to the main thread, while
   `sync_atmosphere_positions`, `sync_cloud_layers`, `sync_ring_positions`,
   `sync_ring_material_uniforms`, `track_selected_body` and
   `apply_camera_flight` only read `BodyRuntime` and can run on a worker before
   it. At the 100 000× slider maximum Earth moves about half its atmosphere
   radius per frame, so the halo and the Venus cloud shell can visibly detach.
   Add `.after(simulation::update_body_positions)` to those systems, as the
   gizmo systems already do for `update_camera_transform`.

3. **Shift-drag pan is undone by body tracking.** — open
   Pan adds to the orbit target (`camera.rs`, `orbit_camera_input`), but
   `track_selected_body` lerps the target back to the body at 8/s whenever a
   body is selected, and every jump selects one. The result is a rubber-band
   pan that snaps back within a few frames. Either clear the selection on pan
   or track a pan offset relative to the body.

4. **Orbit rings are flat circles of a second copy of the semi-major axes.** — open
   `render.rs:draw_orbit_paths` draws circles of `BodySpec::semi_major_axis_au`
   in the ecliptic plane. Pluto's real orbit (e 0.25, i 17°) is up to ~10 AU off
   that ring; Ceres and Vesta sit 7–11° out of plane. The `types.rs` values also
   disagree with the elements in `ephemeris.rs` (Pluto 39.482 vs 39.589).
   Sample `orbital_position_au` over each body's own elements instead.

5. **Pluto and Charon spin in the opposite sense to Charon's orbit.** — open
   Charon's orbit is built counter-clockwise about `PLUTO_POLE_SCENE`
   (`simulation.rs`, `satellite_scene_offset` / `charon_relative_scene_offset`),
   which is the right-hand positive pole. Pluto and Charon are given negative
   spin (`types.rs`), while Earth has positive spin and a prograde orbit. In a
   tidally locked pair spin and orbit share a sense, so one sign is wrong.
   Related, needs a visual check: by derivation from Bevy's UV sphere (u
   increases counter-clockwise about +Z) and `rotate_local_z`, the global
   negation in `spin_step_radians` makes every ecliptic-pole body rotate
   clockwise seen from ecliptic north, i.e. retrograde. With north up, surface
   features should drift left to right. If they drift right to left, drop the
   negation and Pluto's sign becomes correct as it stands.

6. **Two implementations of the satellite orbits have diverged.** — open
   `ephemeris.rs:fallback_satellite_position_au` phases moons on Unix days,
   while `simulation.rs:satellite_scene_offset`, the one that places them, uses
   elapsed days. The ephemeris result for the five reconstructed bodies is
   computed every frame (with a redundant Kepler solve of the primary each) and
   discarded, and `position_au("IO")` returns a moon ~180° from the rendered
   one. Keep one implementation.

## Low priority

7. **Startup hitch:** `util.rs:image_to_rgba8_data` clones the 8K Milky Way
   image and then clones its data again, ~270 MB of transient copies on the main
   thread. Borrow the data for the already-RGBA8 case. — open
8. **Auto-exposure comment contradicts the code:** `setup.rs` says the range is
   widened past the default, but `-2.0..=2.0` narrows Bevy's `-8.0..=8.0`
   default. CLAUDE.md repeats the claim. Decide which is intended. — open
9. **Saturn's rings are not in its equatorial plane:** the ring is tilted 26.73°
   (`types.rs` `RingSpec`, `setup.rs`) while Saturn's `pole_direction` stays
   ecliptic, so the planet spins about a different axis than its rings. Drive
   both from `pole_direction`. — open
10. **Three independent "day zero" clocks:** `SpiceEphemeris.start_unix_days`,
    `base_et` from a whole-second string, and `SimulationEpoch.start_utc`.
    Sub-second apart today, but nothing ties them together. — open
11. **Spin is integrated incrementally**, so "Go to Date" and Backspace leave
    rotation phase untouched. Setting rotation from absolute simulation time
    would make it deterministic. — open
12. **Per-frame churn:** `render.rs:sync_visibility_toggles` writes `Visibility`
    unconditionally every frame (compare first, like
    `sync_asteroid_visibility`). `PointLightShadowMap { size: 2048 }` in
    `mod.rs` is dead since point-light shadows are disabled every frame. — open
13. **Comment and constant nits:** `PLUTO_ELEMENTS` epoch JD 2457588.5 is
    2016-07-19, not 07-31. `solve_kepler`'s "e ≤ 0.2" bound is now exceeded by
    Pluto (0.252). `KM_PER_AU` and `SECONDS_PER_DAY` are defined in both
    `ephemeris.rs` and `types.rs`. — open

## What looked good

Module boundaries are clean and `pub(super)` is respected throughout; the
floating-origin design is applied consistently, including in both shaders; the
Horizons parser and retry logic are well tested; all shell scripts use
`set -euo pipefail`; both GitHub workflows run with read-only permissions.
