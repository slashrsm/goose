//! Live web dashboard for observing a running Goose load test.
//!
//! Enabled by default on `127.0.0.1:5118`. Disable with `--no-dashboard`.

use crate::graph::DashboardTimeseries;
use crate::metrics::{calculate_response_time_percentile, GooseMetrics, MetricsCommand};
use crate::test_plan::TestPlanHistory;
use crate::{AttackPhase, GooseConfiguration};

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures::stream::Stream;
use serde::Serialize;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, RwLock};
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::StreamExt as _;

const INDEX_HTML: &str = include_str!("static/index.html");
const APP_JS: &str = include_str!("static/app.js");
const STYLE_CSS: &str = include_str!("static/style.css");

/// Shared runtime state written by the attack loop and read by HTTP handlers.
#[derive(Clone, Debug)]
pub struct DashboardLiveState {
    pub phase: AttackPhase,
    pub active_users: usize,
    pub maximum_users: usize,
    pub total_users: usize,
    pub duration_secs: usize,
    pub hosts: Vec<String>,
    pub display_metrics: bool,
    pub metrics_history: Vec<TestPlanHistory>,
}

impl Default for DashboardLiveState {
    fn default() -> Self {
        Self {
            phase: AttackPhase::Idle,
            active_users: 0,
            maximum_users: 0,
            total_users: 0,
            duration_secs: 0,
            hosts: Vec::new(),
            display_metrics: true,
            metrics_history: Vec::new(),
        }
    }
}

/// Handle passed into the dashboard HTTP server.
#[derive(Clone)]
pub struct DashboardHandle {
    pub live: Arc<RwLock<DashboardLiveState>>,
    /// Metrics command sender; replaced when the attack loop resets the processor.
    pub metrics_cmd_tx: Arc<RwLock<flume::Sender<MetricsCommand>>>,
    pub configuration: GooseConfiguration,
}

#[derive(Serialize)]
struct StatusResponse {
    phase: String,
    elapsed_secs: usize,
    active_users: usize,
    maximum_users: usize,
    total_users: usize,
    hosts: Vec<String>,
    display_metrics: bool,
    dashboard: DashboardBindInfo,
}

#[derive(Serialize)]
struct DashboardBindInfo {
    host: String,
    port: u16,
}

#[derive(Serialize)]
struct ErrorEntry {
    message: String,
    count: usize,
}

#[derive(Serialize)]
struct RequestRow {
    method: String,
    name: String,
    path: String,
    success_count: usize,
    fail_count: usize,
    requests_per_second: f64,
    failures_per_second: f64,
    response_time_average: f64,
    response_time_minimum: usize,
    response_time_maximum: usize,
    p50: usize,
    p95: usize,
    p99: usize,
}

#[derive(Serialize)]
struct TransactionRow {
    scenario: String,
    name: String,
    times_run: usize,
    fails: usize,
    transactions_per_second: f64,
    fail_per_second: f64,
    response_time_average: f64,
    response_time_minimum: usize,
    response_time_maximum: usize,
}

#[derive(Serialize)]
struct ScenarioRow {
    name: String,
    users: usize,
    times_run: usize,
    scenarios_per_second: f64,
    response_time_average: f64,
    response_time_minimum: usize,
    response_time_maximum: usize,
}

#[derive(Serialize)]
struct SummaryResponse {
    phase: String,
    elapsed_secs: usize,
    active_users: usize,
    maximum_users: usize,
    total_users: usize,
    hosts: Vec<String>,
    display_metrics: bool,
    total_requests: usize,
    total_failures: usize,
    requests_per_second: f64,
    failures_per_second: f64,
    fail_percent: f64,
    response_time_average: f64,
    response_time_minimum: usize,
    response_time_maximum: usize,
    p50: usize,
    p75: usize,
    p90: usize,
    p95: usize,
    p99: usize,
    top_errors: Vec<ErrorEntry>,
    requests: Vec<RequestRow>,
    transactions: Vec<TransactionRow>,
    scenarios: Vec<ScenarioRow>,
    timeseries: DashboardTimeseries,
}

/// Shared pieces needed by the attack loop after the dashboard starts.
pub struct DashboardRuntime {
    pub live: Arc<RwLock<DashboardLiveState>>,
    pub metrics_cmd_tx: Arc<RwLock<flume::Sender<MetricsCommand>>>,
}

impl std::fmt::Debug for DashboardRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DashboardRuntime").finish_non_exhaustive()
    }
}

/// Spawn the dashboard HTTP server if enabled.
pub async fn setup_dashboard(
    configuration: &mut GooseConfiguration,
    defaults_dashboard_host: Option<String>,
    defaults_dashboard_port: Option<u16>,
    metrics_cmd_tx: flume::Sender<MetricsCommand>,
) -> Option<DashboardRuntime> {
    if configuration.no_dashboard {
        return None;
    }

    if configuration.dashboard_host.is_empty() {
        configuration.dashboard_host = defaults_dashboard_host
            .unwrap_or_else(|| "127.0.0.1".to_string());
    }
    if configuration.dashboard_port == 0 {
        configuration.dashboard_port = defaults_dashboard_port.unwrap_or_else(|| {
            crate::DEFAULT_DASHBOARD_PORT
                .parse()
                .expect("invalid DEFAULT_DASHBOARD_PORT")
        });
    }

    let live = Arc::new(RwLock::new(DashboardLiveState::default()));
    let metrics_cmd_tx = Arc::new(RwLock::new(metrics_cmd_tx));

    let bind = format!(
        "{}:{}",
        configuration.dashboard_host, configuration.dashboard_port
    );
    let listener = match TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => {
            error!("[dashboard]: failed to bind {bind}: {e}");
            return None;
        }
    };
    if let Ok(local) = listener.local_addr() {
        configuration.dashboard_port = local.port();
        debug!("[dashboard]: bound address {local}");
    }
    // Prefer a clickable loopback URL in logs.
    let display_host = if configuration.dashboard_host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        configuration.dashboard_host.as_str()
    };
    info!(
        "[dashboard]: listening on http://{}:{}/",
        display_host, configuration.dashboard_port
    );

    let handle = DashboardHandle {
        live: live.clone(),
        metrics_cmd_tx: metrics_cmd_tx.clone(),
        configuration: configuration.clone(),
    };

    let app = router(handle);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("[dashboard]: server error: {e}");
        }
    });

    Some(DashboardRuntime {
        live,
        metrics_cmd_tx,
    })
}

/// Point the dashboard at a new metrics processor command channel (after reset).
pub async fn update_metrics_cmd_tx(
    runtime: &DashboardRuntime,
    metrics_cmd_tx: flume::Sender<MetricsCommand>,
) {
    *runtime.metrics_cmd_tx.write().await = metrics_cmd_tx;
}

fn router(handle: DashboardHandle) -> Router {
    Router::new()
        .route("/", get(index_html))
        .route("/static/app.js", get(app_js))
        .route("/static/style.css", get(style_css))
        .route("/api/v1/status", get(api_status))
        .route("/api/v1/metrics", get(api_metrics))
        .route("/api/v1/summary", get(api_summary))
        .route("/api/v1/timeseries", get(api_timeseries))
        .route("/api/v1/stream", get(api_stream))
        .with_state(handle)
}

async fn index_html() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_js() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
        APP_JS,
    )
        .into_response()
}

async fn style_css() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/css; charset=utf-8"),
        )],
        STYLE_CSS,
    )
        .into_response()
}

fn phase_name(phase: &AttackPhase) -> String {
    match phase {
        AttackPhase::Idle => "Idle".to_string(),
        AttackPhase::Increase => "Increase".to_string(),
        AttackPhase::Maintain => "Maintain".to_string(),
        AttackPhase::Decrease => "Decrease".to_string(),
        AttackPhase::Shutdown => "Shutdown".to_string(),
    }
}

async fn api_status(State(handle): State<DashboardHandle>) -> Json<StatusResponse> {
    let live = handle.live.read().await;
    Json(StatusResponse {
        phase: phase_name(&live.phase),
        elapsed_secs: live.duration_secs,
        active_users: live.active_users,
        maximum_users: live.maximum_users,
        total_users: live.total_users,
        hosts: live.hosts.clone(),
        display_metrics: live.display_metrics,
        dashboard: DashboardBindInfo {
            host: handle.configuration.dashboard_host.clone(),
            port: handle.configuration.dashboard_port,
        },
    })
}

async fn fetch_snapshot(
    handle: &DashboardHandle,
) -> Option<(GooseMetrics, crate::graph::GraphData)> {
    let live = handle.live.read().await;
    let (respond_tx, respond_rx) = oneshot::channel();
    let cmd = MetricsCommand::GetSnapshot {
        duration: live.duration_secs,
        total_users: live.total_users,
        maximum_users: live.maximum_users,
        history: live.metrics_history.clone(),
        respond: respond_tx,
    };
    drop(live);
    let tx = handle.metrics_cmd_tx.read().await.clone();
    if tx.send(cmd).is_err() {
        return None;
    }
    respond_rx.await.ok()
}

async fn api_metrics(State(handle): State<DashboardHandle>) -> Response {
    match fetch_snapshot(&handle).await {
        Some((metrics, _)) => Json(metrics).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "metrics unavailable").into_response(),
    }
}

async fn api_timeseries(State(handle): State<DashboardHandle>) -> Response {
    match fetch_snapshot(&handle).await {
        Some((_, graph)) => Json(graph.to_dashboard_timeseries()).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "timeseries unavailable").into_response(),
    }
}

async fn api_summary(State(handle): State<DashboardHandle>) -> Response {
    match build_summary(&handle).await {
        Some(summary) => Json(summary).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, "summary unavailable").into_response(),
    }
}

async fn build_summary(handle: &DashboardHandle) -> Option<SummaryResponse> {
    let (metrics, graph) = fetch_snapshot(handle).await?;
    let live = handle.live.read().await;
    let duration = live.duration_secs.max(1) as f64;

    let mut total_requests = 0usize;
    let mut total_failures = 0usize;
    let mut combined_times: BTreeMap<usize, usize> = BTreeMap::new();
    let mut total_time = 0usize;
    let mut time_counter = 0usize;
    let mut minimum_time = 0usize;
    let mut maximum_time = 0usize;

    let mut requests = Vec::new();
    for metric in metrics.requests.values() {
        let successes = metric.success_count;
        let fails = metric.fail_count;
        let count = successes + fails;
        total_requests += count;
        total_failures += fails;

        let raw = &metric.raw_data;
        total_time += raw.total_time;
        time_counter += raw.counter;
        if raw.counter > 0 {
            if minimum_time == 0 || raw.minimum_time < minimum_time {
                minimum_time = raw.minimum_time;
            }
            if raw.maximum_time > maximum_time {
                maximum_time = raw.maximum_time;
            }
            for (t, c) in &raw.times {
                *combined_times.entry(*t).or_insert(0) += c;
            }
        }

        let avg = if raw.counter > 0 {
            raw.total_time as f64 / raw.counter as f64
        } else {
            0.0
        };
        requests.push(RequestRow {
            method: metric.method.to_string(),
            name: metric.path.clone(),
            path: metric.path.clone(),
            success_count: successes,
            fail_count: fails,
            requests_per_second: count as f64 / duration,
            failures_per_second: fails as f64 / duration,
            response_time_average: avg,
            response_time_minimum: raw.minimum_time,
            response_time_maximum: raw.maximum_time,
            p50: calculate_response_time_percentile(&raw.times, raw.counter, raw.minimum_time, raw.maximum_time, 0.5),
            p95: calculate_response_time_percentile(&raw.times, raw.counter, raw.minimum_time, raw.maximum_time, 0.95),
            p99: calculate_response_time_percentile(&raw.times, raw.counter, raw.minimum_time, raw.maximum_time, 0.99),
        });
    }
    requests.sort_by(|a, b| {
        (b.success_count + b.fail_count).cmp(&(a.success_count + a.fail_count))
    });

    let mut transactions = Vec::new();
    for scenario_txns in &metrics.transactions {
        for txn in scenario_txns {
            let runs = txn.success_count + txn.fail_count;
            let avg = if runs > 0 {
                txn.total_time as f64 / runs as f64
            } else {
                0.0
            };
            transactions.push(TransactionRow {
                scenario: txn.scenario_name.to_string(),
                name: txn.transaction_name.name_for_transaction().to_string(),
                times_run: runs,
                fails: txn.fail_count,
                transactions_per_second: runs as f64 / duration,
                fail_per_second: txn.fail_count as f64 / duration,
                response_time_average: avg,
                response_time_minimum: txn.min_time,
                response_time_maximum: txn.max_time,
            });
        }
    }

    let mut scenarios = Vec::new();
    for scenario in &metrics.scenarios {
        let avg = if scenario.counter > 0 {
            scenario.total_time as f64 / scenario.counter as f64
        } else {
            0.0
        };
        scenarios.push(ScenarioRow {
            name: scenario.name.to_string(),
            users: scenario.users.len(),
            times_run: scenario.counter,
            scenarios_per_second: scenario.counter as f64 / duration,
            response_time_average: avg,
            response_time_minimum: scenario.min_time,
            response_time_maximum: scenario.max_time,
        });
    }

    let mut top_errors: Vec<ErrorEntry> = metrics
        .errors
        .iter()
        .map(|(message, err)| ErrorEntry {
            message: message.clone(),
            count: err.occurrences,
        })
        .collect();
    top_errors.sort_by(|a, b| b.count.cmp(&a.count));
    top_errors.truncate(50);

    let avg = if time_counter > 0 {
        total_time as f64 / time_counter as f64
    } else {
        0.0
    };

    Some(SummaryResponse {
        phase: phase_name(&live.phase),
        elapsed_secs: live.duration_secs,
        active_users: live.active_users,
        maximum_users: live.maximum_users,
        total_users: live.total_users,
        hosts: live.hosts.clone(),
        display_metrics: live.display_metrics,
        total_requests,
        total_failures,
        requests_per_second: total_requests as f64 / duration,
        failures_per_second: total_failures as f64 / duration,
        fail_percent: if total_requests > 0 {
            (total_failures as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        },
        response_time_average: avg,
        response_time_minimum: minimum_time,
        response_time_maximum: maximum_time,
        p50: calculate_response_time_percentile(
            &combined_times,
            time_counter,
            minimum_time,
            maximum_time,
            0.5,
        ),
        p75: calculate_response_time_percentile(
            &combined_times,
            time_counter,
            minimum_time,
            maximum_time,
            0.75,
        ),
        p90: calculate_response_time_percentile(
            &combined_times,
            time_counter,
            minimum_time,
            maximum_time,
            0.9,
        ),
        p95: calculate_response_time_percentile(
            &combined_times,
            time_counter,
            minimum_time,
            maximum_time,
            0.95,
        ),
        p99: calculate_response_time_percentile(
            &combined_times,
            time_counter,
            minimum_time,
            maximum_time,
            0.99,
        ),
        top_errors,
        requests,
        transactions,
        scenarios,
        timeseries: graph.to_dashboard_timeseries(),
    })
}

async fn api_stream(
    State(handle): State<DashboardHandle>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(1))).then(
        move |_| {
            let handle = handle.clone();
            async move {
                match build_summary(&handle).await {
                    Some(summary) => {
                        let data = serde_json::to_string(&summary).unwrap_or_else(|_| "{}".into());
                        Ok(Event::default().event("summary").data(data))
                    }
                    None => Ok(Event::default()
                        .event("error")
                        .data("{\"error\":\"metrics unavailable\"}")),
                }
            }
        },
    );
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Push attack-loop state into the dashboard shared snapshot.
pub async fn update_live_state(
    live: &Arc<RwLock<DashboardLiveState>>,
    phase: AttackPhase,
    active_users: usize,
    maximum_users: usize,
    total_users: usize,
    duration_secs: usize,
    hosts: impl IntoIterator<Item = String>,
    display_metrics: bool,
    metrics_history: Vec<TestPlanHistory>,
) {
    let mut state = live.write().await;
    state.phase = phase;
    state.active_users = active_users;
    state.maximum_users = maximum_users;
    state.total_users = total_users;
    state.duration_secs = duration_secs;
    state.hosts = hosts.into_iter().collect();
    state.display_metrics = display_metrics;
    state.metrics_history = metrics_history;
}
