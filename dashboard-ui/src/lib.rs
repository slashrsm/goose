//! Goose live dashboard UI (Yew → WebAssembly).
//!
//! Talks to the embedded Axum API (`/api/v1/*`) served by the Goose process.

use gloo_net::http::Request;
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{EventSource, MessageEvent};
use yew::prelude::*;

// ---------------------------------------------------------------------------
// API types (mirror server JSON from goose::dashboard)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct DashboardTimeseries {
    #[serde(default)]
    pub elapsed_secs: Vec<usize>,
    #[serde(default)]
    pub requests_per_second: Vec<u32>,
    #[serde(default)]
    pub errors_per_second: Vec<u32>,
    #[serde(default)]
    pub average_response_time_ms: Vec<f32>,
    #[serde(default)]
    pub users: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ErrorEntry {
    pub message: String,
    pub count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct RequestRow {
    pub method: String,
    pub name: String,
    pub path: String,
    pub success_count: usize,
    pub fail_count: usize,
    pub requests_per_second: f64,
    pub failures_per_second: f64,
    pub response_time_average: f64,
    pub response_time_minimum: usize,
    pub response_time_maximum: usize,
    pub p50: usize,
    pub p95: usize,
    pub p99: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct TransactionRow {
    pub scenario: String,
    pub name: String,
    pub times_run: usize,
    pub fails: usize,
    pub transactions_per_second: f64,
    pub fail_per_second: f64,
    pub response_time_average: f64,
    pub response_time_minimum: usize,
    pub response_time_maximum: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ScenarioRow {
    pub name: String,
    pub users: usize,
    pub times_run: usize,
    pub scenarios_per_second: f64,
    pub response_time_average: f64,
    pub response_time_minimum: usize,
    pub response_time_maximum: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SummaryResponse {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub elapsed_secs: usize,
    #[serde(default)]
    pub active_users: usize,
    #[serde(default)]
    pub maximum_users: usize,
    #[serde(default)]
    pub total_users: usize,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub display_metrics: bool,
    #[serde(default)]
    pub total_requests: usize,
    #[serde(default)]
    pub total_failures: usize,
    #[serde(default)]
    pub requests_per_second: f64,
    #[serde(default)]
    pub failures_per_second: f64,
    #[serde(default)]
    pub fail_percent: f64,
    #[serde(default)]
    pub response_time_average: f64,
    #[serde(default)]
    pub response_time_minimum: usize,
    #[serde(default)]
    pub response_time_maximum: usize,
    #[serde(default)]
    pub p50: usize,
    #[serde(default)]
    pub p75: usize,
    #[serde(default)]
    pub p90: usize,
    #[serde(default)]
    pub p95: usize,
    #[serde(default)]
    pub p99: usize,
    #[serde(default)]
    pub top_errors: Vec<ErrorEntry>,
    #[serde(default)]
    pub requests: Vec<RequestRow>,
    #[serde(default)]
    pub transactions: Vec<TransactionRow>,
    #[serde(default)]
    pub scenarios: Vec<ScenarioRow>,
    #[serde(default)]
    pub timeseries: DashboardTimeseries,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConnMode {
    Disconnected,
    Polling,
    Live,
}

impl ConnMode {
    fn as_str(self) -> &'static str {
        match self {
            ConnMode::Disconnected => "disconnected",
            ConnMode::Polling => "polling",
            ConnMode::Live => "live",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Requests,
    Transactions,
    Scenarios,
    Errors,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fmt_f64(n: f64, digits: usize) -> String {
    if n.is_nan() {
        return "—".into();
    }
    format!("{n:.digits$}")
}

fn phase_class(phase: &str) -> String {
    format!("badge phase-{}", phase.to_lowercase())
}

/// SVG line chart with legend, axis lines, grid, and tick labels (ECharts-like).
#[function_component(LineChart)]
fn line_chart(props: &LineChartProps) -> Html {
    let w = 480.0_f64;
    let h = 220.0_f64;
    let margin_l = 48.0;
    let margin_r = 16.0;
    let margin_t = 28.0;
    let margin_b = 36.0;
    let plot_w = w - margin_l - margin_r;
    let plot_h = h - margin_t - margin_b;

    let values = &props.values;
    let xs = &props.x_labels;
    let color = props.color;
    let series_name = props.series_name;

    if values.is_empty() {
        return html! {
            <svg class="spark" viewBox={format!("0 0 {w} {h}")}>
                <text x="24" y="110" fill="#8b9bb4" font-size="12">{"no data yet"}</text>
            </svg>
        };
    }

    let min_v = 0.0_f64.min(values.iter().cloned().fold(f64::INFINITY, f64::min));
    let max_raw = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let max_v = if max_raw <= min_v { min_v + 1.0 } else { max_raw * 1.05 };
    let span = (max_v - min_v).max(1e-9);
    let n = values.len();
    let n_f = (n.saturating_sub(1)).max(1) as f64;

    let mut line = String::new();
    let mut area = String::new();
    for (i, v) in values.iter().enumerate() {
        let x = margin_l + (i as f64) / n_f * plot_w;
        let y = margin_t + plot_h - (v - min_v) / span * plot_h;
        if i == 0 {
            line.push_str(&format!("M {x:.2} {y:.2}"));
            area.push_str(&format!("M {x:.2} {:.2}", margin_t + plot_h));
            area.push_str(&format!(" L {x:.2} {y:.2}"));
        } else {
            line.push_str(&format!(" L {x:.2} {y:.2}"));
            area.push_str(&format!(" L {x:.2} {y:.2}"));
        }
        if i + 1 == n {
            area.push_str(&format!(" L {x:.2} {:.2} Z", margin_t + plot_h));
        }
    }

    // Horizontal grid + Y ticks (5 lines)
    let y_ticks = 5;
    let mut grid_lines = Vec::new();
    let mut y_labels = Vec::new();
    for t in 0..=y_ticks {
        let frac = t as f64 / y_ticks as f64;
        let y = margin_t + plot_h - frac * plot_h;
        let val = min_v + frac * span;
        grid_lines.push(html! {
            <line
                x1={margin_l.to_string()} y1={format!("{y:.2}")}
                x2={(margin_l + plot_w).to_string()} y2={format!("{y:.2}")}
                stroke="#2d3a4d" stroke-dasharray="4 4" stroke-width="1"
            />
        });
        y_labels.push(html! {
            <text
                x={(margin_l - 6.0).to_string()}
                y={format!("{y:.2}")}
                text-anchor="end"
                dominant-baseline="middle"
                fill="#8b9bb4"
                font-size="10"
                font-family="ui-monospace, monospace"
            >
                { format_tick(val) }
            </text>
        });
    }

    // X ticks (up to ~6 labels)
    let x_tick_count = 6.min(n).max(1);
    let mut x_labels_el = Vec::new();
    for t in 0..x_tick_count {
        let idx = if x_tick_count == 1 {
            0
        } else {
            t * (n - 1) / (x_tick_count - 1)
        };
        let x = margin_l + (idx as f64) / n_f * plot_w;
        let label = xs
            .get(idx)
            .cloned()
            .unwrap_or_else(|| idx.to_string());
        x_labels_el.push(html! {
            <text
                x={format!("{x:.2}")}
                y={(h - 10.0).to_string()}
                text-anchor="middle"
                fill="#8b9bb4"
                font-size="10"
                font-family="ui-monospace, monospace"
            >
                { label }
            </text>
        });
    }

    let axis_color = "#2d3a4d";
    html! {
        <svg class="spark" viewBox={format!("0 0 {w} {h}")} preserveAspectRatio="xMidYMid meet">
            // Legend
            <rect x={(w - 120.0).to_string()} y="6" width="12" height="12" fill={color} rx="2" />
            <text x={(w - 104.0).to_string()} y="16" fill="#8b9bb4" font-size="11">{ series_name }</text>
            // Plot background grid
            { for grid_lines }
            { for y_labels }
            // Axes
            <line
                x1={margin_l.to_string()} y1={margin_t.to_string()}
                x2={margin_l.to_string()} y2={(margin_t + plot_h).to_string()}
                stroke={axis_color} stroke-width="1.5"
            />
            <line
                x1={margin_l.to_string()} y1={(margin_t + plot_h).to_string()}
                x2={(margin_l + plot_w).to_string()} y2={(margin_t + plot_h).to_string()}
                stroke={axis_color} stroke-width="1.5"
            />
            // Area + line
            <path d={area} fill={color} opacity="0.2" />
            <path d={line} fill="none" stroke={color} stroke-width="2" />
            // X labels + axis name
            { for x_labels_el }
            <text
                x={(margin_l + plot_w / 2.0).to_string()}
                y={h.to_string()}
                text-anchor="middle"
                fill="#8b9bb4"
                font-size="10"
            >{"elapsed (s)"}</text>
        </svg>
    }
}

fn format_tick(v: f64) -> String {
    if v.abs() >= 1000.0 {
        format!("{:.0}", v)
    } else if v.abs() >= 10.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.2}", v)
    }
}

#[derive(Properties, PartialEq)]
struct LineChartProps {
    values: Vec<f64>,
    /// X-axis labels (e.g. elapsed seconds as strings); may be shorter/longer than values.
    x_labels: Vec<String>,
    color: &'static str,
    series_name: &'static str,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[function_component(App)]
pub fn app() -> Html {
    let summary = use_state(SummaryResponse::default);
    let conn = use_state(|| ConnMode::Disconnected);
    let tab = use_state(|| Tab::Requests);

    // Prefer SSE; fall back to polling on error.
    {
        let summary = summary.clone();
        let conn = conn.clone();
        use_effect_with((), move |_| {
            let mut cleanup: Option<EventSource> = None;
            if let Ok(es) = EventSource::new("/api/v1/stream") {
                let summary_es = summary.clone();
                let conn_es = conn.clone();
                // Server emits named SSE events: `event: summary`
                let on_summary = Closure::wrap(Box::new(move |e: MessageEvent| {
                    if let Some(text) = e.data().as_string() {
                        if let Ok(parsed) = serde_json::from_str::<SummaryResponse>(&text) {
                            summary_es.set(parsed);
                            conn_es.set(ConnMode::Live);
                        }
                    }
                }) as Box<dyn FnMut(_)>);
                let _ = es.add_event_listener_with_callback(
                    "summary",
                    on_summary.as_ref().unchecked_ref(),
                );
                on_summary.forget();

                let summary_poll = summary.clone();
                let conn_poll = conn.clone();
                let es_err = es.clone();
                let onerror = Closure::wrap(Box::new(move |_e: web_sys::Event| {
                    es_err.close();
                    conn_poll.set(ConnMode::Polling);
                    // Fire-and-forget poll loop
                    let summary_poll = summary_poll.clone();
                    let conn_poll = conn_poll.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            match Request::get("/api/v1/summary").send().await {
                                Ok(resp) if resp.ok() => {
                                    if let Ok(parsed) = resp.json::<SummaryResponse>().await {
                                        summary_poll.set(parsed);
                                        conn_poll.set(ConnMode::Polling);
                                    }
                                }
                                _ => conn_poll.set(ConnMode::Disconnected),
                            }
                            gloo_timers::future::TimeoutFuture::new(1000).await;
                        }
                    });
                }) as Box<dyn FnMut(_)>);
                es.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                onerror.forget();
                cleanup = Some(es);
            } else {
                conn.set(ConnMode::Polling);
                let summary_poll = summary.clone();
                let conn_poll = conn.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    loop {
                        match Request::get("/api/v1/summary").send().await {
                            Ok(resp) if resp.ok() => {
                                if let Ok(parsed) = resp.json::<SummaryResponse>().await {
                                    summary_poll.set(parsed);
                                    conn_poll.set(ConnMode::Polling);
                                }
                            }
                            _ => conn_poll.set(ConnMode::Disconnected),
                        }
                        gloo_timers::future::TimeoutFuture::new(1000).await;
                    }
                });
            }
            move || {
                if let Some(es) = cleanup {
                    es.close();
                }
            }
        });
    }

    let s = (*summary).clone();
    let err_count: usize = s.top_errors.iter().map(|e| e.count).sum();
    let x_labels: Vec<String> = s
        .timeseries
        .elapsed_secs
        .iter()
        .map(|s| s.to_string())
        .collect();
    let rps_series: Vec<f64> = s
        .timeseries
        .requests_per_second
        .iter()
        .map(|v| *v as f64)
        .collect();
    let users_series: Vec<f64> = s.timeseries.users.iter().map(|v| *v as f64).collect();
    let rt_series: Vec<f64> = s
        .timeseries
        .average_response_time_ms
        .iter()
        .map(|v| *v as f64)
        .collect();
    let x_rps = x_labels.clone();
    let x_users = x_labels.clone();
    let x_rt = x_labels;

    let active_tab = *tab;
    let set_tab = |t: Tab| {
        let tab = tab.clone();
        Callback::from(move |_| tab.set(t))
    };

    html! {
        <>
            <header class="top">
                <div class="brand">
                    <span class="logo">{"Goose"}</span>
                    <span class="subtitle">{"Live load test dashboard (WASM)"}</span>
                </div>
                <div class="meta">
                    <span class={phase_class(&s.phase)}>{ &s.phase }</span>
                    <span class="mono">{ format!("{}s", s.elapsed_secs) }</span>
                    <span class="hosts">{ s.hosts.join(", ") }</span>
                    <span class={format!("conn {}", conn.as_str())}>{ conn.as_str() }</span>
                </div>
            </header>

            <section class="kpis">
                <div class="kpi"><div class="label">{"Active users"}</div><div class="value">{ s.active_users }</div></div>
                <div class="kpi"><div class="label">{"RPS"}</div><div class="value">{ fmt_f64(s.requests_per_second, 2) }</div></div>
                <div class="kpi"><div class="label">{"Fail %"}</div><div class="value">{ format!("{}%", fmt_f64(s.fail_percent, 2)) }</div></div>
                <div class="kpi"><div class="label">{"p95 (ms)"}</div><div class="value">{ s.p95 }</div></div>
                <div class="kpi"><div class="label">{"Requests"}</div><div class="value">{ s.total_requests }</div></div>
                <div class="kpi"><div class="label">{"Errors"}</div><div class="value">{ err_count }</div></div>
            </section>

            <section class="charts">
                <div class="chart-card">
                    <div class="chart-title">{"Requests / second"}</div>
                    <LineChart values={rps_series} x_labels={x_rps} color={"#3d9cf0"} series_name={"RPS"} />
                </div>
                <div class="chart-card">
                    <div class="chart-title">{"Active users"}</div>
                    <LineChart values={users_series} x_labels={x_users} color={"#3dd68c"} series_name={"Users"} />
                </div>
                <div class="chart-card">
                    <div class="chart-title">{"Avg response time (ms)"}</div>
                    <LineChart values={rt_series} x_labels={x_rt} color={"#f5a524"} series_name={"Avg RT (ms)"} />
                </div>
            </section>

            <nav class="tabs">
                <button class={if active_tab == Tab::Requests {"tab active"} else {"tab"}} onclick={set_tab(Tab::Requests)}>{"Requests"}</button>
                <button class={if active_tab == Tab::Transactions {"tab active"} else {"tab"}} onclick={set_tab(Tab::Transactions)}>{"Transactions"}</button>
                <button class={if active_tab == Tab::Scenarios {"tab active"} else {"tab"}} onclick={set_tab(Tab::Scenarios)}>{"Scenarios"}</button>
                <button class={if active_tab == Tab::Errors {"tab active"} else {"tab"}} onclick={set_tab(Tab::Errors)}>{"Errors"}</button>
            </nav>

            <section class="panels">
                if active_tab == Tab::Requests {
                    <div class="panel active">
                        <table>
                            <thead><tr>
                                <th>{"Method"}</th><th>{"Name"}</th><th>{"# Req"}</th><th>{"# Fail"}</th>
                                <th>{"RPS"}</th><th>{"Fail/s"}</th><th>{"Avg"}</th><th>{"Min"}</th><th>{"Max"}</th>
                                <th>{"p50"}</th><th>{"p95"}</th><th>{"p99"}</th>
                            </tr></thead>
                            <tbody>
                                { for s.requests.iter().map(|r| html! {
                                    <tr>
                                        <td>{ &r.method }</td>
                                        <td>{ &r.name }</td>
                                        <td>{ r.success_count + r.fail_count }</td>
                                        <td>{ r.fail_count }</td>
                                        <td>{ fmt_f64(r.requests_per_second, 2) }</td>
                                        <td>{ fmt_f64(r.failures_per_second, 2) }</td>
                                        <td>{ fmt_f64(r.response_time_average, 1) }</td>
                                        <td>{ r.response_time_minimum }</td>
                                        <td>{ r.response_time_maximum }</td>
                                        <td>{ r.p50 }</td>
                                        <td>{ r.p95 }</td>
                                        <td>{ r.p99 }</td>
                                    </tr>
                                }) }
                            </tbody>
                        </table>
                    </div>
                }
                if active_tab == Tab::Transactions {
                    <div class="panel active">
                        <table>
                            <thead><tr>
                                <th>{"Scenario"}</th><th>{"Transaction"}</th><th>{"# Run"}</th><th>{"# Fail"}</th>
                                <th>{"TPS"}</th><th>{"Fail/s"}</th><th>{"Avg"}</th><th>{"Min"}</th><th>{"Max"}</th>
                            </tr></thead>
                            <tbody>
                                { for s.transactions.iter().map(|t| html! {
                                    <tr>
                                        <td>{ &t.scenario }</td>
                                        <td>{ &t.name }</td>
                                        <td>{ t.times_run }</td>
                                        <td>{ t.fails }</td>
                                        <td>{ fmt_f64(t.transactions_per_second, 2) }</td>
                                        <td>{ fmt_f64(t.fail_per_second, 2) }</td>
                                        <td>{ fmt_f64(t.response_time_average, 1) }</td>
                                        <td>{ t.response_time_minimum }</td>
                                        <td>{ t.response_time_maximum }</td>
                                    </tr>
                                }) }
                            </tbody>
                        </table>
                    </div>
                }
                if active_tab == Tab::Scenarios {
                    <div class="panel active">
                        <table>
                            <thead><tr>
                                <th>{"Scenario"}</th><th>{"Users"}</th><th>{"# Run"}</th><th>{"Scen/s"}</th>
                                <th>{"Avg"}</th><th>{"Min"}</th><th>{"Max"}</th>
                            </tr></thead>
                            <tbody>
                                { for s.scenarios.iter().map(|sc| html! {
                                    <tr>
                                        <td>{ &sc.name }</td>
                                        <td>{ sc.users }</td>
                                        <td>{ sc.times_run }</td>
                                        <td>{ fmt_f64(sc.scenarios_per_second, 2) }</td>
                                        <td>{ fmt_f64(sc.response_time_average, 1) }</td>
                                        <td>{ sc.response_time_minimum }</td>
                                        <td>{ sc.response_time_maximum }</td>
                                    </tr>
                                }) }
                            </tbody>
                        </table>
                    </div>
                }
                if active_tab == Tab::Errors {
                    <div class="panel active">
                        <table>
                            <thead><tr><th>{"Count"}</th><th>{"Error"}</th></tr></thead>
                            <tbody>
                                { for s.top_errors.iter().map(|e| html! {
                                    <tr>
                                        <td>{ e.count }</td>
                                        <td style="white-space:normal;font-family:var(--sans)">{ &e.message }</td>
                                    </tr>
                                }) }
                            </tbody>
                        </table>
                    </div>
                }
            </section>

            <footer>
                {"Running metrics are approximate while the test is in progress. UI is Rust/WebAssembly (Yew). "}
                {"Use "}<code>{"--report-file"}</code>{" for an archival HTML report when the test finishes."}
            </footer>
        </>
    }
}

/// Entry point called from JS glue after the WASM module loads.
#[wasm_bindgen(start)]
pub fn main() {
    yew::Renderer::<App>::new().render();
}
