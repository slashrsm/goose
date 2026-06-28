#!/usr/bin/env bash
# Build the Yew WASM dashboard and copy artifacts into src/dashboard/static/wasm/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
UI="$ROOT/dashboard-ui"
OUT="$ROOT/src/dashboard/static/wasm"
TARGET="${CARGO_TARGET_DIR:-$UI/target}"

echo "[dashboard-ui] building wasm32 release…"
cargo build --manifest-path "$UI/Cargo.toml" --target wasm32-unknown-unknown --release

WASM_IN="$TARGET/wasm32-unknown-unknown/release/goose_dashboard_ui.wasm"
if [[ ! -f "$WASM_IN" ]]; then
  # workspace-style target dir at repo root
  WASM_IN="$ROOT/target/wasm32-unknown-unknown/release/goose_dashboard_ui.wasm"
fi
if [[ ! -f "$WASM_IN" ]]; then
  echo "error: wasm artifact not found" >&2
  exit 1
fi

mkdir -p "$OUT"
echo "[dashboard-ui] wasm-bindgen → $OUT"
wasm-bindgen "$WASM_IN" \
  --target web \
  --no-typescript \
  --out-dir "$OUT" \
  --out-name goose_dashboard_ui

# Tiny loader used by index.html (stable name for include_str / routes)
cat > "$OUT/bootstrap.js" << 'JS'
import init from './goose_dashboard_ui.js';
init().catch((err) => {
  console.error('Goose dashboard WASM failed to load', err);
  document.body.innerHTML = '<pre style="color:#f31260;padding:2rem">Failed to load WASM dashboard UI. Rebuild with scripts/build-dashboard-ui.sh</pre>';
});
JS

echo "[dashboard-ui] done:"
ls -la "$OUT"
