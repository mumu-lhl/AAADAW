#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
output=${1:-"$repo_root/target/aaadaw-desktop-smoke.png"}

# The managed Linux runtime libraries are staged alongside target when the
# host development packages are unavailable.
runtime_pkgconfig="$repo_root/target/runtime-pkgconfig"
if [[ -d "$runtime_pkgconfig" ]]; then
  export PKG_CONFIG_PATH="$runtime_pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
fi

# Large test runs can leave several hundred megabytes of GUI-crate incremental
# state and failed-link object files behind. Reclaim only those disposable
# artifacts when a fresh desktop link would otherwise start below 1 GiB free.
available_kb=$(df -Pk "$repo_root" | awk 'NR == 2 { print $4 }')
if [[ "$available_kb" =~ ^[0-9]+$ ]] && (( available_kb < 1000000 )); then
  incremental="$repo_root/target/debug/incremental"
  if [[ -d "$incremental" ]]; then
    find "$incremental" -mindepth 1 -maxdepth 1 -type d -name 'aaadaw-*' -exec rm -rf -- {} +
  fi
  dependencies="$repo_root/target/debug/deps"
  if [[ -d "$dependencies" ]]; then
    find "$dependencies" -maxdepth 1 -type f \
      \( -name 'aaadaw-*.rcgu.o' -o -name 'aaadaw-*.tmp*' \) -delete
  fi
fi

# Keep the large debug information in dependency artifacts for normal Rust
# development, but avoid writing hundreds of megabytes of DWARF into the GUI
# executable. This makes a fresh desktop link practical in disk-constrained
# workspaces without changing runtime code generation.
cargo rustc --locked --manifest-path "$repo_root/Cargo.toml" \
  --package aaadaw --bin aaadaw -- \
  -C link-arg=-Wl,--strip-debug

exec "$repo_root/scripts/reaper_parity/run-aaadaw-desktop-smoke.sh" \
  "$repo_root/target/debug/aaadaw" "$output"
