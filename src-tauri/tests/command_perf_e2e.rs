//! E2E performance tests for all instrumented commands.
//!
//! Tests exercise local commands (file/config operations) and verify
//! that timing data is properly collected in the PerfRegistry.

use clawpal::commands::perf::{
    get_perf_report, get_perf_timings, get_process_metrics, init_perf_clock, record_timing,
};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn setup() {
    init_perf_clock();
    let _ = get_perf_timings();
}

fn temp_data_dir() -> std::path::PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("clawpal-perf-e2e-{}", ts));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

#[test]
fn registry_collects_samples() {
    setup();
    record_timing("test_command_a", 42);
    record_timing("test_command_b", 100);
    record_timing("test_command_a", 55);

    let samples = get_perf_timings().expect("should return timings");
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].name, "test_command_a");
    assert_eq!(samples[0].elapsed_ms, 42);
    assert_eq!(samples[1].name, "test_command_b");
    assert_eq!(samples[2].name, "test_command_a");

    let empty = get_perf_timings().expect("should return empty");
    assert!(empty.is_empty());
}

#[test]
fn report_aggregates_correctly() {
    setup();
    record_timing("cmd_fast", 10);
    record_timing("cmd_fast", 20);
    record_timing("cmd_fast", 30);
    record_timing("cmd_slow", 500);
    record_timing("cmd_slow", 600);

    let report = get_perf_report().expect("should return report");
    let fast = &report["cmd_fast"];
    assert_eq!(fast["count"], 3);
    assert_eq!(fast["p50_ms"], 20);
    let slow = &report["cmd_slow"];
    assert_eq!(slow["count"], 2);
}

#[test]
fn local_config_commands_record_timing() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let data_dir = temp_data_dir();
    unsafe {
        std::env::set_var("CLAWPAL_DATA_DIR", &data_dir);
    }
    setup();

    use clawpal::commands::{
        get_app_preferences, list_ssh_hosts, local_openclaw_config_exists, read_app_log,
    };

    let _ = local_openclaw_config_exists("/nonexistent".to_string());
    let _ = list_ssh_hosts();
    let _ = get_app_preferences();
    let _ = read_app_log(Some(10));

    let samples = get_perf_timings().expect("should have timings");
    let names: Vec<&str> = samples.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"local_openclaw_config_exists"));
    assert!(names.contains(&"list_ssh_hosts"));

    for s in &samples {
        assert!(
            s.elapsed_ms < 100,
            "{} took {}ms — should be < 100ms for local ops",
            s.name,
            s.elapsed_ms
        );
    }
}

#[test]
fn z_local_perf_report_for_ci() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let data_dir = temp_data_dir();
    unsafe {
        std::env::set_var("CLAWPAL_DATA_DIR", &data_dir);
    }
    setup();

    use clawpal::commands::{
        get_app_preferences, list_ssh_hosts, local_openclaw_config_exists, read_app_log,
        read_error_log,
    };

    let commands: Vec<(&str, Box<dyn Fn()>)> = vec![
        (
            "local_openclaw_config_exists",
            Box::new(|| {
                let _ = local_openclaw_config_exists("/tmp".to_string());
            }),
        ),
        (
            "list_ssh_hosts",
            Box::new(|| {
                let _ = list_ssh_hosts();
            }),
        ),
        (
            "get_app_preferences",
            Box::new(|| {
                let _ = get_app_preferences();
            }),
        ),
        (
            "read_app_log",
            Box::new(|| {
                let _ = read_app_log(Some(10));
            }),
        ),
        (
            "read_error_log",
            Box::new(|| {
                let _ = read_error_log(Some(10));
            }),
        ),
    ];

    for (_, cmd_fn) in &commands {
        for _ in 0..5 {
            cmd_fn();
        }
    }

    let report = get_perf_report().expect("should return report");
    println!();
    println!("PERF_REPORT_START");
    for (name, _) in &commands {
        if let Some(stats) = report.get(*name) {
            println!(
                "LOCAL_CMD:{}:count={}:p50={}:p95={}:max={}:avg={}",
                name,
                stats["count"],
                stats["p50_ms"],
                stats["p95_ms"],
                stats["max_ms"],
                stats["avg_ms"],
            );
        }
    }

    let metrics = get_process_metrics().expect("metrics");
    let rss_mb = metrics.rss_bytes as f64 / (1024.0 * 1024.0);
    println!("PROCESS:rss_mb={:.1}", rss_mb);
    println!("PROCESS:platform={}", metrics.platform);
    println!("PERF_REPORT_END");
}
