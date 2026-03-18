//! E2E tests for performance metrics instrumentation.
//!
//! These tests verify that:
//! 1. `get_process_metrics` returns valid data
//! 2. `trace_command` tracks timing correctly
//! 3. Memory readings are within expected bounds
//! 4. The perf clock measures uptime correctly

use clawpal::commands::perf::{
    get_process_metrics, init_perf_clock, trace_command, uptime_ms, PerfSample, ProcessMetrics,
};
use std::thread;
use std::time::Duration;

// ── Gate: get_process_metrics returns sane values ──

#[test]
fn process_metrics_returns_valid_pid() {
    init_perf_clock();
    let metrics = get_process_metrics().expect("should return metrics");
    assert_eq!(metrics.pid, std::process::id());
}

#[test]
fn process_metrics_rss_within_bounds() {
    init_perf_clock();
    let metrics = get_process_metrics().expect("should return metrics");

    // Test process should use at least 1 MB and less than 80 MB (the target)
    let rss_mb = metrics.rss_bytes as f64 / (1024.0 * 1024.0);
    assert!(
        rss_mb > 1.0,
        "RSS too low: {:.1} MB — likely measurement error",
        rss_mb
    );
    assert!(rss_mb < 80.0, "RSS exceeds 80 MB target: {:.1} MB", rss_mb);
}

#[test]
fn process_metrics_platform_is_set() {
    init_perf_clock();
    let metrics = get_process_metrics().expect("should return metrics");
    assert!(!metrics.platform.is_empty(), "platform should be set");
    // Should be one of the supported platforms
    assert!(
        ["linux", "macos", "windows"].contains(&metrics.platform.as_str()),
        "unexpected platform: {}",
        metrics.platform
    );
}

#[test]
fn process_metrics_uptime_is_positive() {
    init_perf_clock();
    // Small sleep so uptime is measurably > 0
    thread::sleep(Duration::from_millis(5));
    let metrics = get_process_metrics().expect("should return metrics");
    assert!(
        metrics.uptime_secs > 0.0,
        "uptime should be positive: {}",
        metrics.uptime_secs
    );
}

// ── Gate: trace_command timing ──

#[test]
fn trace_command_measures_fast_operation() {
    init_perf_clock();
    let (result, elapsed_ms) = trace_command("test_fast_op", || {
        let x = 2 + 2;
        x
    });
    assert_eq!(result, 4);
    // A trivial operation should complete in well under 100ms (the local threshold)
    assert!(
        elapsed_ms < 100,
        "fast operation took {}ms — should be < 100ms",
        elapsed_ms
    );
}

#[test]
fn trace_command_measures_slow_operation() {
    init_perf_clock();
    let (_, elapsed_ms) = trace_command("test_slow_op", || {
        thread::sleep(Duration::from_millis(150));
    });
    // Should measure at least 100ms
    assert!(
        elapsed_ms >= 100,
        "slow operation measured as {}ms — should be >= 100ms",
        elapsed_ms
    );
    // But shouldn't be wildly over (allow up to 500ms for CI scheduling jitter)
    assert!(
        elapsed_ms < 500,
        "slow operation measured as {}ms — excessive",
        elapsed_ms
    );
}

// ── Gate: uptime clock ──

#[test]
fn uptime_ms_increases_over_time() {
    init_perf_clock();
    let t1 = uptime_ms();
    thread::sleep(Duration::from_millis(20));
    let t2 = uptime_ms();
    assert!(t2 > t1, "uptime should increase: {} vs {}", t1, t2);
    let delta = t2 - t1;
    assert!(
        delta >= 10, // allow some scheduling variance
        "uptime delta too small: {}ms (expected ~20ms)",
        delta
    );
}

// ── Gate: memory stability under repeated calls ──

#[test]
fn memory_stable_across_repeated_metrics_calls() {
    init_perf_clock();

    // Take initial measurement
    let initial = get_process_metrics().expect("first call");
    let initial_rss = initial.rss_bytes;

    // Call get_process_metrics 100 times to ensure no memory leak in the measurement itself
    for _ in 0..100 {
        let _ = get_process_metrics();
    }

    let after = get_process_metrics().expect("last call");
    let growth = after.rss_bytes.saturating_sub(initial_rss);
    let growth_mb = growth as f64 / (1024.0 * 1024.0);

    // Memory growth from 100 metric reads should be negligible (< 5 MB)
    assert!(
        growth_mb < 5.0,
        "Memory grew {:.1} MB after 100 metrics calls — potential leak",
        growth_mb
    );
}

// ── Gate: PerfSample struct serialization ──

#[test]
fn perf_sample_serializes_correctly() {
    let sample = PerfSample {
        name: "test_command".to_string(),
        elapsed_ms: 42,
        timestamp: 1710000000000,
        exceeded_threshold: false,
    };

    let json = serde_json::to_string(&sample).expect("should serialize");
    assert!(json.contains("\"name\":\"test_command\""));
    assert!(json.contains("\"elapsedMs\":42")); // camelCase
    assert!(json.contains("\"exceededThreshold\":false"));
}

// ── Metrics reporter: outputs structured data for CI comment ──

#[test]
fn z_report_metrics_for_ci() {
    init_perf_clock();

    // Process metrics
    let metrics = get_process_metrics().expect("should return metrics");
    let rss_mb = metrics.rss_bytes as f64 / (1024.0 * 1024.0);
    let vms_mb = metrics.vms_bytes as f64 / (1024.0 * 1024.0);

    // Command timing: measure a batch of get_process_metrics calls
    let iterations = 50;
    let mut times: Vec<u64> = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let (_, elapsed) = trace_command("get_process_metrics", || {
            let _ = get_process_metrics();
        });
        times.push(elapsed);
    }
    times.sort();
    let p50 = times[times.len() / 2];
    let p95 = times[(times.len() as f64 * 0.95) as usize];
    let max = *times.last().unwrap_or(&0);

    // Output structured lines for CI to parse
    // Format: METRIC:<name>=<value>
    println!();
    println!("METRIC:rss_mb={:.1}", rss_mb);
    println!("METRIC:vms_mb={:.1}", vms_mb);
    println!("METRIC:pid={}", metrics.pid);
    println!("METRIC:platform={}", metrics.platform);
    println!("METRIC:uptime_secs={:.2}", metrics.uptime_secs);
    println!("METRIC:cmd_p50_ms={}", p50);
    println!("METRIC:cmd_p95_ms={}", p95);
    println!("METRIC:cmd_max_ms={}", max);
    println!("METRIC:rss_limit_mb=80");
    println!("METRIC:cmd_p95_limit_ms=100");
}
