//! Prometheus metrics endpoint.
//!
//! Exposes process and application metrics in Prometheus text exposition format
//! at `GET /api/metrics`.

use actix_web::{HttpResponse, get};
use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Global counters (atomics — lock-free, no external crate needed)
// ---------------------------------------------------------------------------

/// Total HTTP requests served (incremented by middleware).
pub static HTTP_REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Total HTTP request errors (4xx + 5xx).
pub static HTTP_ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Start time of the process.
static mut START_INSTANT: Option<Instant> = None;
static mut START_UNIX: f64 = 0.0;

/// Call once at startup to record the process start time.
pub fn init_start_time() {
    unsafe {
        START_INSTANT = Some(Instant::now());
        START_UNIX = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
    }
}

fn uptime_secs() -> f64 {
    unsafe {
        START_INSTANT
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }
}

fn start_unix() -> f64 {
    unsafe { START_UNIX }
}

// ---------------------------------------------------------------------------
// /proc-based process metrics (Linux)
// ---------------------------------------------------------------------------

/// Read /proc/self/stat for CPU times.
fn proc_cpu_seconds() -> (f64, f64) {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return (0.0, 0.0);
    };
    let fields: Vec<&str> = stat.split_whitespace().collect();
    if fields.len() < 15 {
        return (0.0, 0.0);
    }
    let ticks_per_sec = 100.0_f64; // sysconf(_SC_CLK_TCK), nearly always 100 on Linux
    let utime: f64 = fields[13].parse().unwrap_or(0.0) / ticks_per_sec;
    let stime: f64 = fields[14].parse().unwrap_or(0.0) / ticks_per_sec;
    (utime, stime)
}

/// Read /proc/self/status for memory info (VmRSS, VmSize).
fn proc_memory() -> (u64, u64) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return (0, 0);
    };
    let mut rss_bytes = 0u64;
    let mut vsize_bytes = 0u64;
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("VmRSS:") {
            rss_bytes = parse_kb(val) * 1024;
        } else if let Some(val) = line.strip_prefix("VmSize:") {
            vsize_bytes = parse_kb(val) * 1024;
        }
    }
    (rss_bytes, vsize_bytes)
}

/// Read /proc/self/fd to count open file descriptors.
fn proc_open_fds() -> u64 {
    std::fs::read_dir("/proc/self/fd")
        .map(|entries| entries.count() as u64)
        .unwrap_or(0)
}

/// Read /proc/self/status for thread count.
fn proc_threads() -> u64 {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("Threads:") {
            return val.trim().parse().unwrap_or(0);
        }
    }
    0
}

fn parse_kb(s: &str) -> u64 {
    s.trim().trim_end_matches("kB").trim().parse().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Prometheus metrics endpoint.
///
/// Exposes process metrics (CPU, memory, uptime, file descriptors, threads)
/// and application metrics (GTFS data statistics, HTTP counters).
#[utoipa::path(
    get,
    path = "/api/metrics",
    responses(
        (status = 200, description = "Prometheus text exposition metrics"),
    ),
    tag = "Status"
)]
#[get("/api/metrics")]
pub async fn get_metrics() -> HttpResponse {
    let mut out = String::with_capacity(2048);

    // --- Process metrics ---
    let (utime, stime) = proc_cpu_seconds();
    let cpu_total = utime + stime;
    let (rss, vsize) = proc_memory();
    let fds = proc_open_fds();
    let threads = proc_threads();

    write_metric(
        &mut out,
        "process_cpu_seconds_total",
        "Total user and system CPU time spent in seconds.",
        "counter",
        cpu_total,
    );
    write_metric(
        &mut out,
        "process_resident_memory_bytes",
        "Resident memory size in bytes.",
        "gauge",
        rss as f64,
    );
    write_metric(
        &mut out,
        "process_virtual_memory_bytes",
        "Virtual memory size in bytes.",
        "gauge",
        vsize as f64,
    );
    write_metric(
        &mut out,
        "process_open_fds",
        "Number of open file descriptors.",
        "gauge",
        fds as f64,
    );
    write_metric(
        &mut out,
        "process_threads",
        "Number of OS threads.",
        "gauge",
        threads as f64,
    );
    write_metric(
        &mut out,
        "process_start_time_seconds",
        "Start time of the process since unix epoch in seconds.",
        "gauge",
        start_unix(),
    );
    write_metric(
        &mut out,
        "process_uptime_seconds",
        "Number of seconds since the process started.",
        "gauge",
        uptime_secs(),
    );

    // --- HTTP metrics ---
    write_metric(
        &mut out,
        "glove_http_requests_total",
        "Total number of HTTP requests served.",
        "counter",
        HTTP_REQUESTS_TOTAL.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "glove_http_errors_total",
        "Total number of HTTP error responses (4xx + 5xx).",
        "counter",
        HTTP_ERRORS_TOTAL.load(Ordering::Relaxed) as f64,
    );

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4; charset=utf-8")
        .body(out)
}

/// Write a single Prometheus metric with HELP, TYPE, and value lines.
fn write_metric(out: &mut String, name: &str, help: &str, metric_type: &str, value: f64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} {metric_type}");
    if value == value.floor() && value.abs() < 1e15 {
        let _ = writeln!(out, "{name} {}", value as i64);
    } else {
        let _ = writeln!(out, "{name} {value:.6}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_metric_integer() {
        let mut out = String::new();
        write_metric(&mut out, "my_counter", "A counter.", "counter", 42.0);
        assert!(out.contains("# HELP my_counter A counter."));
        assert!(out.contains("# TYPE my_counter counter"));
        assert!(out.contains("my_counter 42"));
    }

    #[test]
    fn write_metric_float() {
        let mut out = String::new();
        write_metric(&mut out, "my_gauge", "A gauge.", "gauge", 3.14);
        assert!(out.contains("my_gauge 3.14"));
    }

    #[test]
    fn write_metric_zero() {
        let mut out = String::new();
        write_metric(&mut out, "zero", "Zero.", "gauge", 0.0);
        assert!(out.contains("zero 0"));
    }

    #[test]
    fn parse_kb_valid() {
        assert_eq!(parse_kb("  1234 kB  "), 1234);
    }

    #[test]
    fn parse_kb_invalid() {
        assert_eq!(parse_kb("abc"), 0);
    }

    #[test]
    fn parse_kb_empty() {
        assert_eq!(parse_kb(""), 0);
    }

    #[test]
    fn proc_cpu_returns_non_negative() {
        let (u, s) = proc_cpu_seconds();
        assert!(u >= 0.0);
        assert!(s >= 0.0);
    }

    #[test]
    fn proc_memory_returns_values() {
        let (rss, vsize) = proc_memory();
        // On Linux, we should get non-zero values for our own process
        if cfg!(target_os = "linux") {
            assert!(rss > 0);
            assert!(vsize > 0);
        }
    }

    #[test]
    fn proc_open_fds_positive() {
        let fds = proc_open_fds();
        // A running process always has at least stdin/stdout/stderr
        assert!(fds >= 3);
    }

    #[test]
    fn proc_threads_positive() {
        let threads = proc_threads();
        assert!(threads >= 1);
    }

    #[test]
    fn init_and_uptime() {
        init_start_time();
        let up = uptime_secs();
        assert!(up >= 0.0);
        let start = start_unix();
        assert!(start > 0.0);
    }

    #[test]
    fn atomic_counters() {
        let before = HTTP_REQUESTS_TOTAL.load(Ordering::Relaxed);
        HTTP_REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);
        assert_eq!(HTTP_REQUESTS_TOTAL.load(Ordering::Relaxed), before + 1);
    }

    #[test]
    fn write_metric_large_integer() {
        let mut out = String::new();
        write_metric(&mut out, "big", "Big value.", "counter", 1_000_000.0);
        assert!(out.contains("big 1000000"));
    }

    #[test]
    fn write_metric_negative() {
        let mut out = String::new();
        write_metric(&mut out, "neg", "Negative.", "gauge", -5.0);
        assert!(out.contains("neg -5"));
    }

    #[test]
    fn parse_kb_with_extra_whitespace() {
        assert_eq!(parse_kb("   2048   kB   "), 2048);
    }

    #[test]
    fn parse_kb_plain_number() {
        assert_eq!(parse_kb("512"), 512);
    }

    #[test]
    fn proc_memory_rss_less_than_vsize() {
        let (rss, vsize) = proc_memory();
        if rss > 0 && vsize > 0 {
            assert!(rss <= vsize);
        }
    }

    #[test]
    fn error_counter_increments() {
        let before = HTTP_ERRORS_TOTAL.load(Ordering::Relaxed);
        HTTP_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
        assert_eq!(HTTP_ERRORS_TOTAL.load(Ordering::Relaxed), before + 1);
    }

    #[actix_web::test]
    async fn get_metrics_returns_prometheus_format() {
        init_start_time();
        let app = actix_web::test::init_service(actix_web::App::new().service(get_metrics)).await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/metrics")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = actix_web::test::read_body(resp).await;
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("# HELP process_cpu_seconds_total"));
        assert!(text.contains("# TYPE process_cpu_seconds_total counter"));
        assert!(text.contains("process_resident_memory_bytes"));
        assert!(text.contains("process_uptime_seconds"));
        assert!(text.contains("glove_http_requests_total"));
        assert!(text.contains("glove_http_errors_total"));
    }
}
