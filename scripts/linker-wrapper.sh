#!/usr/bin/env bash
set -euo pipefail

select_compiler() {
  if [[ -x "/opt/homebrew/opt/llvm/bin/clang" ]]; then
    echo "/opt/homebrew/opt/llvm/bin/clang"
    return
  fi

  if command -v clang >/dev/null 2>&1; then
    command -v clang
    return
  fi

  command -v cc
}

supports_mold() {
  if ! command -v mold >/dev/null 2>&1; then
    return 1
  fi

  local cache_dir="${TMPDIR:-/tmp}/solar-navigator-linker"
  mkdir -p "${cache_dir}"

  local cache_key
  cache_key="$(printf "%s" "${CC_BIN}" | shasum | awk '{print $1}')"
  local cache_file="${cache_dir}/mold-supported-${cache_key}"

  if [[ -f "${cache_file}" ]]; then
    [[ "$(cat "${cache_file}")" == "1" ]]
    return
  fi

  local probe_src="${cache_dir}/probe.c"
  local probe_bin="${cache_dir}/probe-bin"
  cat > "${probe_src}" <<'EOF'
int main(void) { return 0; }
EOF

  if "${CC_BIN}" -fuse-ld=mold "${probe_src}" -o "${probe_bin}" >/dev/null 2>&1; then
    echo "1" > "${cache_file}"
    rm -f "${probe_src}" "${probe_bin}"
    return 0
  fi

  echo "0" > "${cache_file}"
  rm -f "${probe_src}" "${probe_bin}"
  return 1
}

CC_BIN="$(select_compiler)"

if supports_mold; then
  exec "${CC_BIN}" -fuse-ld=mold "$@"
fi

exec "${CC_BIN}" "$@"
