#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
VENDOR_DIR="${PROJECT_ROOT}/vendor/cspice"
CSPICE_DIR="${VENDOR_DIR}/cspice"
URL="https://naif.jpl.nasa.gov/pub/naif/toolkit//C/MacM1_OSX_clang_64bit/packages/cspice.tar.Z"

if [[ -f "${CSPICE_DIR}/lib/libcspice.a" ]]; then
  echo "CSPICE already installed at ${CSPICE_DIR}"
  exit 0
fi

mkdir -p "${VENDOR_DIR}"
cd "${VENDOR_DIR}"

if [[ ! -f cspice.tar.Z ]]; then
  echo "Downloading arm64 CSPICE toolkit..."
  curl -fL "${URL}" -o cspice.tar.Z
fi

echo "Extracting CSPICE toolkit..."
gzip -df cspice.tar.Z
tar -xf cspice.tar
rm -f cspice.tar

if [[ -f "${CSPICE_DIR}/lib/cspice.a" ]]; then
  mv "${CSPICE_DIR}/lib/cspice.a" "${CSPICE_DIR}/lib/libcspice.a"
fi

echo "CSPICE installed at ${CSPICE_DIR}"
