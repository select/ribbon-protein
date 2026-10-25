#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../crates/ribbon-core"
command -v wasm-pack >/dev/null || { echo "Install wasm-pack: https://rustwasm.github.io/wasm-pack/installer/" >&2; exit 1; }
wasm-pack build --target web --out-dir ../../public/wasm --out-name ribbon_core "$@"
