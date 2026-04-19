# Third-Party Notices

Last updated: 2026-04-19

This file documents third-party code, data, and assets used by this project.
It is not legal advice.

## 1. Project License Scope

The project source code in this repository is licensed under the MIT License
(see `LICENSE`), except where a different license is noted for bundled
third-party components.

## 2. Bundled Third-Party Source Code

### `vendor/kiss3d`

- Upstream: <https://github.com/dimforge/kiss3d>
- Local path: `vendor/kiss3d`
- License: BSD 3-Clause
- License text: `vendor/kiss3d/LICENSE`

## 3. Rust Crate Dependencies

Application dependencies are resolved through Cargo (`Cargo.toml` and
`Cargo.lock`) and may use different licenses (for example MIT, Apache-2.0,
BSD, ISC, etc.).

When redistributing binaries, ensure dependency license obligations are met.

## 4. External Toolkits and Data

### CSPICE Toolkit (downloaded)

The following scripts download CSPICE from NAIF:

- `scripts/setup_cspice_macos_arm64.sh`
- `scripts/setup_cspice_linux_x86_64.sh`
- `scripts/setup_cspice_windows_x86_64.ps1`

CSPICE is third-party software and is not covered by this repository's MIT
license. See NAIF-distributed terms in the downloaded toolkit.

### SPICE Kernel Data Files (downloaded)

`scripts/download_spice_kernels.sh` downloads kernels from NAIF/JPL public
endpoints into `assets/spice` (for example `naif0012.tls`, `de440s.bsp`).

These files are third-party data and are not covered by this repository's MIT
license.

## 5. External Texture Assets

Texture files under `assets/textures` may be downloaded from third-party
sources by project scripts:

- `scripts/download_textures_solar_system_scope.sh`
- `scripts/download_textures_minor_bodies_science.sh`

Primary referenced sources include:

- Solar System Scope textures: <https://www.solarsystemscope.com/textures/>
  (their texture page lists CC BY 4.0 at time of writing)
- DLR Dawn GIS mosaics:
  <https://dawngis.dlr.de/data/Ceres/mosaic_ceres.php>
  <https://dawngis.dlr.de/data/Vesta/mosaic_vesta.php>
- USGS Astrogeology / New Horizons products:
  <https://astrogeology.usgs.gov/search/map/pluto_new_horizons_lorri_mvic_global_mosaic_300m>
  <https://astrogeology.usgs.gov/search/map/charon_new_horizons_lorri_mvic_global_mosaic_300m>

These assets are not covered by this repository's MIT license. Before
redistribution, verify the current upstream terms and provide required
attribution.

## 6. Attribution Guidance

For redistribution of binaries or asset packs, include attribution and source
references for external assets. See `ASSET_ATTRIBUTION.md` for a ready-to-use
attribution summary.
