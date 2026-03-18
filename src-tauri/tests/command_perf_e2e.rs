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
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    setup();
    record_timing("test_command_a", 42);
    record_timing("test_command_b", 100);
    record_timing("test_command_a", 55);

    let samples = get_perf_timings().expect("should return timings");
    assert!(
        samples.len() >= 3,
        "expected at least 3 samples, got {}",
        samples.len()
    );
    // Find our test samples (other tests may have added samples concurrently)
    let a_samples: Vec<_> = samples
        .iter()
        .filter(|s| s.name == "test_command_a")
        .collect();
    let b_samples: Vec<_> = samples
        .iter()
        .filter(|s| s.name == "test_command_b")
        .collect();
    assert!(a_samples.len() >= 2, "expected 2+ test_command_a samples");
    assert!(b_samples.len() >= 1, "expected 1+ test_command_b samples");

    // Drain should clear
    let empty = get_perf_timings().expect("should return empty");
    assert!(empty.is_empty());
}

#[test]
fn report_aggregates_correctly() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            s.elapsed_us < 100,
            "{} took {}ms — should be < 100ms for local ops",
            s.name,
            s.elapsed_us
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
                "LOCAL_CMD:{}:count={}:p50_us={}:p95_us={}:max_us={}:avg_us={}",
                name,
                stats["count"],
                stats["p50_us"],
                stats["p95_us"],
                stats["max_us"],
                stats["avg_us"],
            );
        }
    }

    let metrics = get_process_metrics().expect("metrics");
    let rss_mb = metrics.rss_bytes as f64 / (1024.0 * 1024.0);
    println!("PROCESS:rss_mb={:.1}", rss_mb);
    println!("PROCESS:platform={}", metrics.platform);
    println!("PERF_REPORT_END");
}
