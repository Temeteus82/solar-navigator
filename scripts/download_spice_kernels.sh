#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPICE_DIR="${ROOT_DIR}/assets/spice"
mkdir -p "${SPICE_DIR}"

curl -fL "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/lsk/naif0012.tls" \
  -o "${SPICE_DIR}/naif0012.tls"

curl -fL "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/de440s.bsp" \
  -o "${SPICE_DIR}/de440s.bsp"

curl -fL "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/pck00011.tpc" \
  -o "${SPICE_DIR}/pck00011.tpc"

curl -fL "https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/gm_de440.tpc" \
  -o "${SPICE_DIR}/gm_de440.tpc"

echo "SPICE kernels downloaded to ${SPICE_DIR}"
