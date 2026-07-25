#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly repository_root

cd "${repository_root}"
cargo fmt --all --check
cargo check --all-targets
"${repository_root}/scripts/build.sh"
