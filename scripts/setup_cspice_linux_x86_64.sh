#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
VENDOR_DIR="${PROJECT_ROOT}/vendor/cspice"
CSPICE_DIR="${VENDOR_DIR}/cspice"
ARCHIVE_Z="${VENDOR_DIR}/cspice.tar.Z"
ARCHIVE_TAR="${VENDOR_DIR}/cspice.tar"
URL="https://naif.jpl.nasa.gov/pub/naif/toolkit//C/PC_Linux_GCC_64bit/packages/cspice.tar.Z"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This script targets Linux hosts only."
  exit 1
fi

case "$(uname -m)" in
  x86_64|amd64) ;;
  *)
    echo "Unsupported Linux architecture: $(uname -m). Expected x86_64/amd64."
    exit 1
    ;;
esac

if [[ -f "${CSPICE_DIR}/lib/libcspice.a" ]]; then
  echo "CSPICE already installed at ${CSPICE_DIR}"
  exit 0
fi

mkdir -p "${VENDOR_DIR}"
rm -rf "${CSPICE_DIR}"

if command -v curl >/dev/null 2>&1; then
  echo "Downloading Linux x86_64 CSPICE toolkit..."
  curl -fL "${URL}" -o "${ARCHIVE_Z}"
elif command -v wget >/dev/null 2>&1; then
  echo "Downloading Linux x86_64 CSPICE toolkit..."
  wget -O "${ARCHIVE_Z}" "${URL}"
else
  echo "Neither curl nor wget is available; cannot download ${URL}"
  exit 1
fi

echo "Extracting CSPICE toolkit..."
gzip -dc "${ARCHIVE_Z}" > "${ARCHIVE_TAR}"
tar -xf "${ARCHIVE_TAR}" -C "${VENDOR_DIR}"
rm -f "${ARCHIVE_TAR}"

if [[ -f "${CSPICE_DIR}/lib/cspice.a" ]]; then
  mv "${CSPICE_DIR}/lib/cspice.a" "${CSPICE_DIR}/lib/libcspice.a"
fi

if [[ ! -f "${CSPICE_DIR}/include/SpiceUsr.h" ]]; then
  echo "CSPICE install failed: missing header ${CSPICE_DIR}/include/SpiceUsr.h"
  exit 1
fi

if [[ ! -f "${CSPICE_DIR}/lib/libcspice.a" ]]; then
  echo "CSPICE install failed: missing static library ${CSPICE_DIR}/lib/libcspice.a"
  exit 1
fi

echo "CSPICE installed at ${CSPICE_DIR}"
