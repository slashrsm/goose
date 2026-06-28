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
