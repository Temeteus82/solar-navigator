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

# naif.jpl.nasa.gov is a single point of failure for every SPICE-mode CI job,
# and a bare fetch turns one transient connection failure into a red build. Cap
# each attempt with --connect-timeout (an unreachable host otherwise hangs for
# over two minutes before giving up) and retry a few times.
#
# curl's plain --retry only covers timeouts and 4xx/5xx responses, not a
# refused connection; --retry-all-errors closes that gap but has only existed
# since curl 7.71 (2020) and older curls abort on the unknown flag rather than
# ignoring it. Probe for it so this script keeps working on long-lived distros
# that still ship 7.6x.
if command -v curl >/dev/null 2>&1; then
  echo "Downloading Linux x86_64 CSPICE toolkit..."
  CURL_RETRY_FLAGS=(--connect-timeout 30 --retry 5 --retry-delay 5)
  if curl --help all 2>/dev/null | grep -q -- '--retry-all-errors'; then
    CURL_RETRY_FLAGS+=(--retry-all-errors)
  fi
  curl -fL "${CURL_RETRY_FLAGS[@]}" "${URL}" -o "${ARCHIVE_Z}"
elif command -v wget >/dev/null 2>&1; then
  echo "Downloading Linux x86_64 CSPICE toolkit..."
  wget --timeout=30 --tries=5 --waitretry=5 -O "${ARCHIVE_Z}" "${URL}"
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
