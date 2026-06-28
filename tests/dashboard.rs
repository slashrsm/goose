//! Tests for the embedded live web dashboard configuration and CLI surface.

use goose::config::GooseConfiguration;
use gumdrop::Options;

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
    use goose::prelude::*;

    // Ensure GooseDefault variants are accepted (set_default returns Ok).
    let _attack = GooseAttack::initialize()
        .unwrap()
        .set_default(GooseDefault::NoDashboard, true)
        .unwrap()
        .set_default(GooseDefault::DashboardHost, "127.0.0.1")
        .unwrap()
        .set_default(GooseDefault::DashboardPort, 5118)
        .unwrap();
}
