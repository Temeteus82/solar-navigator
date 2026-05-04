# Planet Textures

The app looks for the following files in this folder:

- `sun.jpg`
- `mercury.jpg`
- `venus.jpg`
- `earth.jpg`
- `moon.jpg`
- `mars.jpg`
- `jupiter.jpg`
- `saturn.jpg`
- `uranus.jpg`
- `neptune.jpg`

If files are missing, the app falls back to plain colors.

Additional optional textures:

- `milky_way_8k.jpg` (equirectangular sky texture, useful for a skybox/background sphere)
- `sun_2k_backup.jpg` (backup of the older lower-resolution sun map)
- `ceres.jpg` (Dawn Ceres mosaic)
- `vesta.jpg` (Dawn Vesta mosaic)
- `pluto.jpg` (New Horizons Pluto mosaic)
- `charon.jpg` (New Horizons Charon mosaic)

## Auto-download helper

From project root:

```bash
# macOS / Linux
./scripts/download_textures_solar_system_scope.sh
```

```powershell
# Windows
.\scripts\download_textures_solar_system_scope.ps1
```

This script pulls textures from Solar System Scope public endpoints.
Before shipping or redistributing, verify current attribution/license terms.

Minor-body science textures:

```bash
# macOS / Linux
./scripts/download_textures_minor_bodies_science.sh
```

```powershell
# Windows
.\scripts\download_textures_minor_bodies_science.ps1
```

Heavy full-resolution source products (converted locally to JPEG):

```bash
# macOS / Linux
FULL_RES=1 TARGET_WIDTH=4096 ./scripts/download_textures_minor_bodies_science.sh
```

```powershell
# Windows
.\scripts\download_textures_minor_bodies_science.ps1 -FullRes -TargetWidth 4096
```

## Attribution

Current texture downloads in this project are sourced from Solar System Scope:

- https://www.solarsystemscope.com/textures/
- License listed on that page: CC BY 4.0

If you redistribute binaries or assets, include proper attribution for used textures.
See also:

- `ASSET_ATTRIBUTION.md`
- `THIRD_PARTY_NOTICES.md`

Minor-body texture sources used by the new helper script:

- DLR Dawn GIS Ceres/Vesta mosaic products:
  - https://dawngis.dlr.de/data/Ceres/mosaic_ceres.php
  - https://dawngis.dlr.de/data/Vesta/mosaic_vesta.php
- USGS Astrogeology / New Horizons Pluto and Charon products:
  - https://astrogeology.usgs.gov/search/map/pluto_new_horizons_lorri_mvic_global_mosaic_300m
  - https://astrogeology.usgs.gov/search/map/charon_new_horizons_lorri_mvic_global_mosaic_300m
