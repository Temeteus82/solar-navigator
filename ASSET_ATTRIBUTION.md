# Asset Attribution

Last updated: 2026-04-19

This project can use external planetary textures and SPICE data files that are
not authored by Solar Navigator contributors.

## Texture Sources

### Solar System Scope

- Website: <https://www.solarsystemscope.com/textures/>
- Download script: `scripts/download_textures_solar_system_scope.sh`
- Notes: The Solar System Scope texture page lists CC BY 4.0 at time of
  writing. Verify current terms before redistribution.

Suggested attribution text:

`Contains Solar System Scope textures (solarsystemscope.com), used under terms listed by Solar System Scope.`

### DLR Dawn GIS

- Ceres mosaics: <https://dawngis.dlr.de/data/Ceres/mosaic_ceres.php>
- Vesta mosaics: <https://dawngis.dlr.de/data/Vesta/mosaic_vesta.php>
- Download script: `scripts/download_textures_minor_bodies_science.sh`

### USGS Astrogeology / New Horizons

- Pluto mosaic:
  <https://astrogeology.usgs.gov/search/map/pluto_new_horizons_lorri_mvic_global_mosaic_300m>
- Charon mosaic:
  <https://astrogeology.usgs.gov/search/map/charon_new_horizons_lorri_mvic_global_mosaic_300m>
- Download script: `scripts/download_textures_minor_bodies_science.sh`

## SPICE Data Sources

### NAIF/JPL

- Generic kernels index: <https://naif.jpl.nasa.gov/pub/naif/generic_kernels/>
- Download script: `scripts/download_spice_kernels.sh`

These kernels are third-party data and are not covered by this repository's
MIT license.

## Redistribution Checklist

Before publishing binaries or asset bundles:

1. Confirm source license/terms are still current.
2. Include required attribution text in release notes or docs.
3. Keep links to original data sources.
4. Do not imply endorsement by original data providers.
