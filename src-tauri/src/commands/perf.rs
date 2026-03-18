use super::*;

/// Metrics about the current process, exposed to the frontend and E2E tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    /// Process ID
    pub pid: u32,
    /// Resident Set Size in bytes (physical memory used)
    pub rss_bytes: u64,
    /// Virtual memory size in bytes
    pub vms_bytes: u64,
    /// Process uptime in seconds
    pub uptime_secs: f64,
    /// Platform identifier
    pub platform: String,
}

/// Tracks elapsed time of a named operation and logs it.
/// Returns `(result, elapsed_us)` — elapsed time in **microseconds** for
/// sub-millisecond accuracy on fast local commands.
pub fn trace_command<F, T>(name: &str, f: F) -> (T, u64)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    let elapsed_us = start.elapsed().as_micros() as u64;

    let threshold_us = if name.starts_with("remote_") || name.starts_with("ssh_") {
        2_000_000 // 2s
    } else {
        100_000 // 100ms
    };

    if elapsed_us > threshold_us {
        crate::logging::log_info(&format!(
            "[perf] SLOW {} completed in {}us (threshold: {}us)",
            name, elapsed_us, threshold_us
        ));
    } else {
        crate::logging::log_info(&format!("[perf] {} completed in {}us", name, elapsed_us));
    }

    (result, elapsed_us)
}

/// Single perf sample emitted to the frontend via events or returned directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfSample {
    /// The command or operation name
    pub name: String,
    /// Elapsed time in microseconds
    pub elapsed_us: u64,
    /// Timestamp (Unix millis) when the sample was taken
    pub timestamp: u64,
    /// Whether the command exceeded its latency threshold
    pub exceeded_threshold: bool,
}

static APP_START: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Initialize the start time — call this once during app setup.
pub fn init_perf_clock() {
    // Force lazy evaluation so the clock starts ticking from app init, not first command.
    let _ = *APP_START;
}

/// Get the time since app start in milliseconds.
pub fn uptime_ms() -> u64 {
    APP_START.elapsed().as_millis() as u64
}

#[tauri::command]
pub fn get_process_metrics() -> Result<ProcessMetrics, String> {
    let pid = std::process::id();

    let (rss_bytes, vms_bytes) = read_process_memory(pid)?;

    let uptime_secs = APP_START.elapsed().as_secs_f64();

    Ok(ProcessMetrics {
        pid,
        rss_bytes,
        vms_bytes,
        uptime_secs,
        platform: std::env::consts::OS.to_string(),
    })
}

/// Read memory info for a given PID from the OS.
#[cfg(target_os = "linux")]
fn read_process_memory(pid: u32) -> Result<(u64, u64), String> {
    let status_path = format!("/proc/{}/status", pid);
    let content = fs::read_to_string(&status_path)
        .map_err(|e| format!("Failed to read {}: {}", status_path, e))?;

    let mut rss: u64 = 0;
    let mut vms: u64 = 0;

    for line in content.lines() {
        if line.starts_with("VmRSS:") {
            if let Some(val) = parse_proc_kb(line) {
                rss = val * 1024; // Convert KB to bytes
            }
        } else if line.starts_with("VmSize:") {
            if let Some(val) = parse_proc_kb(line) {
                vms = val * 1024;
            }
        }
    }

    Ok((rss, vms))
}

#[cfg(target_os = "linux")]
fn parse_proc_kb(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1)?.parse::<u64>().ok()
}

#[cfg(target_os = "macos")]
fn read_process_memory(pid: u32) -> Result<(u64, u64), String> {
    // Use `ps` as a portable fallback — mach_task_info requires unsafe FFI
    let output = Command::new("ps")
        .args(["-o", "rss=,vsz=", "-p", &pid.to_string()])
        .output()
        .map_err(|e| format!("Failed to run ps: {}", e))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = text.trim().split_whitespace().collect();
    if parts.len() >= 2 {
        let rss_kb: u64 = parts[0].parse().unwrap_or(0);
        let vms_kb: u64 = parts[1].parse().unwrap_or(0);
        Ok((rss_kb * 1024, vms_kb * 1024))
    } else {
        Err("Failed to parse ps output".to_string())
    }
}

#[cfg(target_os = "windows")]
fn read_process_memory(_pid: u32) -> Result<(u64, u64), String> {
    // Windows: use tasklist /FI to get memory info
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", _pid), "/FO", "CSV", "/NH"])
        .output()
        .map_err(|e| format!("Failed to run tasklist: {}", e))?;

    let text = String::from_utf8_lossy(&output.stdout);
    // CSV format: "name","pid","session","session#","mem usage"
    // mem usage is like "12,345 K"
    for line in text.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() >= 5 {
            let mem_str = fields[4].trim().trim_matches('"');
            let mem_kb: u64 = mem_str
                .replace(" K", "")
                .replace(',', "")
                .trim()
                .parse()
                .unwrap_or(0);
            return Ok((mem_kb * 1024, 0)); // VMS not easily available
        }
    }

    Ok((0, 0))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn read_process_memory(_pid: u32) -> Result<(u64, u64), String> {
    Ok((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_command_returns_result_and_timing() {
        let (result, elapsed) = trace_command("test_noop", || 42);
        assert_eq!(result, 42);
        // Should complete in well under 100ms (100_000us)
        assert!(elapsed < 100_000, "noop took {}us", elapsed);
    }

    #[test]
    fn test_get_process_metrics_returns_valid_data() {
        init_perf_clock();
        let metrics = get_process_metrics().expect("should succeed");
        assert!(metrics.pid > 0);
        assert!(metrics.rss_bytes > 0, "RSS should be non-zero");
        assert!(!metrics.platform.is_empty());
    }

    #[test]
    fn test_uptime_increases() {
        init_perf_clock();
        let t1 = uptime_ms();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2 = uptime_ms();
        assert!(t2 > t1, "uptime should increase: {} vs {}", t1, t2);
    }
}

// ── Global performance registry ──

use std::sync::Arc;

/// Maximum number of samples retained in the ring buffer.
/// Prevents unbounded memory growth from long-running polling commands.
const MAX_PERF_SAMPLES: usize = 4096;

/// Thread-safe ring-buffer registry of command timing samples.
static PERF_REGISTRY: LazyLock<Arc<Mutex<VecDeque<PerfSample>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(VecDeque::with_capacity(MAX_PERF_SAMPLES))));

/// Record a timing sample into the global registry.
/// When the registry is full, the oldest sample is evicted.
pub fn record_timing(name: &str, elapsed_us: u64) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let threshold_us = if name.starts_with("remote_") {
        2_000_000
    } else {
        100_000
    };
    let sample = PerfSample {
        name: name.to_string(),
        elapsed_us,
        timestamp: ts,
        exceeded_threshold: elapsed_us > threshold_us,
    };
    if let Ok(mut reg) = PERF_REGISTRY.lock() {
        if reg.len() >= MAX_PERF_SAMPLES {
            reg.pop_front();
        }
        reg.push_back(sample);
    }
}

/// Get all recorded timing samples and clear the registry.
#[tauri::command]
pub fn get_perf_timings() -> Result<Vec<PerfSample>, String> {
    let mut reg = PERF_REGISTRY.lock().map_err(|e| e.to_string())?;
    let samples: Vec<PerfSample> = reg.drain(..).collect();
    Ok(samples)
}

/// Get a summary report of all recorded timings grouped by command name.
#[tauri::command]
pub fn get_perf_report() -> Result<Value, String> {
    let reg = PERF_REGISTRY.lock().map_err(|e| e.to_string())?;

    let mut by_name: HashMap<String, Vec<u64>> = HashMap::new();
    for s in reg.iter() {
        by_name
            .entry(s.name.clone())
            .or_default()
            .push(s.elapsed_us);
    }

    let mut report = serde_json::Map::new();
    for (name, mut times) in by_name {
        times.sort();
        let count = times.len();
        let sum: u64 = times.iter().sum();
        let p50 = times.get(count / 2).copied().unwrap_or(0);
        let p95 = times
            .get((count as f64 * 0.95) as usize)
            .copied()
            .unwrap_or(0);
        let max = times.last().copied().unwrap_or(0);

        report.insert(
            name,
            json!({
                "count": count,
                "p50_us": p50,
                "p95_us": p95,
                "max_us": max,
                "avg_us": if count > 0 { sum / count as u64 } else { 0 },
            }),
        );
    }

    Ok(Value::Object(report))
}
