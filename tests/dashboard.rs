//! Tests for the embedded live web dashboard.

mod common;

use goose::config::GooseConfiguration;
use goose::prelude::*;
use gumdrop::Options;
use httpmock::{Method::GET, MockServer};

const INDEX_PATH: &str = "/";

async fn get_index(user: &mut GooseUser) -> TransactionResult {
    let _goose = user.get(INDEX_PATH).await?;
    Ok(())
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

#[test]
fn test_no_dashboard_flag() {
    let configuration = GooseConfiguration::parse_args_default(&["--no-dashboard"])
        .expect("parse --no-dashboard");
    assert!(configuration.no_dashboard);
}

#[test]
fn test_dashboard_cli_options_exist() {
    let configuration = GooseConfiguration::parse_args_default(&[
        "--dashboard-host",
        "127.0.0.1",
        "--dashboard-port",
        "5118",
    ])
    .expect("parse dashboard options");
    assert_eq!(configuration.dashboard_host, "127.0.0.1");
    assert_eq!(configuration.dashboard_port, 5118);
    assert!(!configuration.no_dashboard);
}

#[test]
fn test_dashboard_defaults_programmatic() {
    let _attack = GooseAttack::initialize()
        .unwrap()
        .set_default(GooseDefault::NoDashboard, true)
        .unwrap()
        .set_default(GooseDefault::DashboardHost, "127.0.0.1")
        .unwrap()
        .set_default(GooseDefault::DashboardPort, 5118)
        .unwrap();
}

/// Regression: with the dashboard enabled, `reset_run_state` must not deadlock
/// waiting on the metrics processor (extra `metrics_cmd_tx` held by the UI).
/// If the handoff order is wrong, this test hangs until the harness times out.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_users_spawn_with_dashboard_enabled() {
    let server = MockServer::start();
    let _mock = server.mock(|when, then| {
        when.method(GET).path(INDEX_PATH);
        then.status(200).body("ok");
    });

    let port = free_port().to_string();
    let configuration = common::build_configuration(
        &server,
        vec![
            "--users",
            "2",
            "--increase-rate",
            "10",
            "--run-time",
            "2",
            "--no-telnet",
            "--no-websocket",
            "--dashboard-host",
            "127.0.0.1",
            "--dashboard-port",
            port.as_str(),
            "--co-mitigation",
            "disabled",
            "--no-reset-metrics",
        ],
    );

    let goose_attack = GooseAttack::initialize_with_config(configuration)
        .unwrap()
        .register_scenario(
            scenario!("LoadTest").register_transaction(transaction!(get_index).set_name("index")),
        );

    let metrics = tokio::time::timeout(std::time::Duration::from_secs(30), goose_attack.execute())
        .await
        .expect("execute timed out — likely metrics-processor deadlock with dashboard")
        .expect("execute failed");

    assert!(
        !metrics.requests.is_empty(),
        "expected request metrics; users likely never spawned"
    );
}
