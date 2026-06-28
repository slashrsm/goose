#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ELM_DIR="$ROOT/dashboard-elm"
OUT_DIR="$ROOT/src/dashboard/static/elm"
TMP="${TMPDIR:-/tmp}/goose-elm-$$.js"

echo "[dashboard-elm] elm make --optimize…"
(cd "$ELM_DIR" && elm make src/Main.elm --optimize --output="$TMP")

mkdir -p "$OUT_DIR"
# Append SSE bootstrap that inits Elm and feeds the sseSummary port
{
  cat "$TMP"
  cat << 'JS'

// Goose dashboard bootstrap: mount Elm + optional EventSource → port
(function () {
  function start() {
    var app = Elm.Main.init({ node: document.getElementById("elm-app") });
    if (!app || !app.ports || !app.ports.sseSummary) {
      return;
    }
    if (!window.EventSource) {
      return;
    }
    var es = new EventSource("/api/v1/stream");
    es.addEventListener("summary", function (ev) {
      try {
        app.ports.sseSummary.send(ev.data);
      } catch (e) { /* ignore */ }
    });
    es.onerror = function () {
      try { es.close(); } catch (e2) { /* ignore */ }
    };
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
JS
} > "$OUT_DIR/app.js"

echo "[dashboard-elm] wrote $OUT_DIR/app.js ($(wc -c < "$OUT_DIR/app.js") bytes)"
rm -f "$TMP"
