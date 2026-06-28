# Live Web Dashboard

Goose serves an embedded live web dashboard so you can observe a running load test in a browser without extra tooling.

By default the dashboard listens on `127.0.0.1:5118` (loopback only). Open [http://127.0.0.1:5118/](http://127.0.0.1:5118/) while a test is running.

## Configuration

| Option | Default | Description |
| --- | --- | --- |
| `--dashboard-host HOST` | `127.0.0.1` | Bind address for the dashboard HTTP server |
| `--dashboard-port PORT` | `5118` | TCP port for the dashboard |
| `--no-dashboard` | off | Disable the dashboard entirely |

Programmatic defaults: [`GooseDefault::DashboardHost`](https://docs.rs/goose/*/goose/config/enum.GooseDefault.html#variant.DashboardHost), [`GooseDefault::DashboardPort`](https://docs.rs/goose/*/goose/config/enum.GooseDefault.html#variant.DashboardPort), and [`GooseDefault::NoDashboard`](https://docs.rs/goose/*/goose/config/enum.GooseDefault.html#variant.NoDashboard).

Bind `0.0.0.0` only on trusted networks; the dashboard is unauthenticated in this version.

## What you see

- Attack phase, elapsed time, active users, and target hosts
- KPI cards: RPS, failure percentage, latency percentiles (p50–p99), totals
- Charts: requests/second, active users, average response time (from the same per-second series used by HTML reports)
- Tables for requests, transactions, scenarios, and error summary

The UI prefers **Server-Sent Events** (`GET /api/v1/stream`) and falls back to polling `GET /api/v1/summary` every second.

## HTTP API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/` | Dashboard UI |
| `GET` | `/api/v1/status` | Phase, users, hosts, bind info |
| `GET` | `/api/v1/summary` | Compact KPIs, tables, timeseries (UI hot path) |
| `GET` | `/api/v1/metrics` | Full [`GooseMetrics`](https://docs.rs/goose/*/goose/metrics/struct.GooseMetrics.html) JSON (same idea as controller `metricsjson`) |
| `GET` | `/api/v1/timeseries` | Per-second RPS / errors / users / avg latency |
| `GET` | `/api/v1/stream` | SSE stream of `summary` events |

## Notes

- Like the telnet and WebSocket controllers, the dashboard is intended for **standalone** Goose processes (not Gaggle manager/worker mode).
- Running metrics are approximate while the test is in progress; use `--report-file` for an archival report when the test finishes.
- Disabling metrics (`--no-metrics`) still serves status, but summary tables stay empty.

## Example

```bash
cargo run --release --example umami -- \
  --host http://localhost/ -u20 -r5 -t2m --no-reset-metrics
# Log line: [dashboard]: listening on http://127.0.0.1:5118/
```

## Frontend selection (compile time)

The dashboard **HTTP API is always the same**; only the browser UI is chosen with Cargo features
(enable **exactly one**):

| Feature | Frontend | Notes |
| --- | --- | --- |
| `dashboard-js` (**default**) | Vanilla JS + [ECharts](https://echarts.apache.org) (CDN) | Legend, axes, grid, tooltips; no WASM toolchain |
| `dashboard-wasm` | [Yew](https://yew.rs/) → WebAssembly | Rust UI; SVG charts with legend, axes, and grid |
| `dashboard-elm` | [Elm](https://elm-lang.org/) 0.19 | Typed FP UI; SVG charts; `scripts/build-dashboard-elm.sh` |

```bash
# Default (JS + ECharts)
cargo build --release --example umami

# Explicit JS only (disable defaults if you had enabled wasm in .cargo/config, etc.)
cargo build --release --example umami --no-default-features --features cookies,dashboard-js

# WASM UI — additive is enough; if both features are on, WASM is used
./scripts/build-dashboard-ui.sh
cargo build --release --example umami --features dashboard-wasm
# equivalent explicit form:
cargo build --release --example umami --no-default-features --features cookies,dashboard-wasm
```

`dashboard-js` is in **default** features. Adding `--features dashboard-wasm` therefore enables
**both** unless you pass `--no-default-features`. That is supported: **Precedence when several features are on: `dashboard-wasm` > `dashboard-elm` > `dashboard-js`.**

## WebAssembly UI rebuild

Artifacts under `src/dashboard/static/wasm/` are embedded only when `dashboard-wasm` is enabled.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126
./scripts/build-dashboard-ui.sh
cargo build --release --no-default-features --features cookies,dashboard-wasm
```

The `wasm-bindgen` CLI version must match the crate used by `dashboard-ui` (pinned to `0.2.126`).


## Elm UI rebuild

```bash
# requires `elm` 0.19.1 on PATH
./scripts/build-dashboard-elm.sh
cargo build --release --features dashboard-elm
```
