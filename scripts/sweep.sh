#!/usr/bin/env bash
# Prune stale build artefacts with cargo-sweep.
#
# Install once:  cargo install cargo-sweep
#
# Usage:
#   ./scripts/sweep.sh          # remove artefacts older than 7 days (default)
#   ./scripts/sweep.sh 14       # remove artefacts older than 14 days
#   ./scripts/sweep.sh stamp    # stamp the current build as "in use"
#   ./scripts/sweep.sh all      # remove ALL artefacts (equivalent to cargo clean)
set -euo pipefail

DAYS="${1:-7}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}/.."

if ! command -v cargo-sweep &>/dev/null; then
  echo "cargo-sweep not found. Install it with:"
  echo "  cargo install cargo-sweep"
  exit 1
fi

case "${DAYS}" in
  stamp)
    echo "Stamping current artefacts as in-use..."
    cargo sweep --stamp
    ;;
  all)
    echo "Removing all build artefacts..."
    cargo sweep --time 0
    ;;
  *)
    echo "Removing artefacts older than ${DAYS} days..."
    cargo sweep --time "${DAYS}"
    ;;
esac

echo "Done. Current target size:"
du -sh target 2>/dev/null || true
