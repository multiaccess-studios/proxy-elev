#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly repository_root

cd "${repository_root}"
export TRUNK_TOOLS_TAILWINDCSS="${TRUNK_TOOLS_TAILWINDCSS:-4.1.18}"
export XDG_CACHE_HOME="${XDG_CACHE_HOME:-${repository_root}/.cache}"
export TMPDIR="${TMPDIR:-${repository_root}/.tmp}"
if [[ -n "${NO_COLOR:-}" ]]; then
  export NO_COLOR=true
fi
mkdir -p "${XDG_CACHE_HOME}/trunk" "${TMPDIR}"

cargo run --bin prepare -- \
  ./netrunner-cards-json \
  ./printing-manifest.toml \
  ./src/manifest.ron
trunk build --release --dist publish --minify -- ./src/index.html
