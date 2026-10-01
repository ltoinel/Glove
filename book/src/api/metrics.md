# Metrics

```
GET /api/metrics
```

Returns server metrics in Prometheus text exposition format (`text/plain; version=0.0.4`). Process metrics are read from `/proc`, so they are reported as `0` on non-Linux hosts.

## Available Metrics

### Process Metrics
- `process_cpu_seconds_total` *(counter)* — Total user and system CPU time, in seconds
- `process_resident_memory_bytes` *(gauge)* — Resident memory (RSS) in bytes
- `process_virtual_memory_bytes` *(gauge)* — Virtual memory size in bytes
- `process_open_fds` *(gauge)* — Number of open file descriptors
- `process_threads` *(gauge)* — Number of OS threads
- `process_start_time_seconds` *(gauge)* — Process start time, Unix epoch seconds
- `process_uptime_seconds` *(gauge)* — Seconds since the process started

### HTTP Metrics
- `glove_http_requests_total` *(counter)* — Total number of HTTP requests served
- `glove_http_errors_total` *(counter)* — Total number of HTTP error responses (4xx, 5xx)

GTFS statistics are not exported as metrics; read them from [`GET /api/gtfs/status`](./status.md#gtfs-status).

## Example

```bash
curl http://localhost:8080/api/metrics
```

```
# HELP process_cpu_seconds_total Total user and system CPU time spent in seconds.
# TYPE process_cpu_seconds_total counter
process_cpu_seconds_total 12.640000
# HELP process_resident_memory_bytes Resident memory size in bytes.
# TYPE process_resident_memory_bytes gauge
process_resident_memory_bytes 335790080
# HELP process_virtual_memory_bytes Virtual memory size in bytes.
# TYPE process_virtual_memory_bytes gauge
process_virtual_memory_bytes 1042432000
# HELP process_open_fds Number of open file descriptors.
# TYPE process_open_fds gauge
process_open_fds 49
# HELP process_threads Number of OS threads.
# TYPE process_threads gauge
process_threads 11
# HELP process_start_time_seconds Start time of the process since unix epoch in seconds.
# TYPE process_start_time_seconds gauge
process_start_time_seconds 1790895550.590277
# HELP process_uptime_seconds Number of seconds since the process started.
# TYPE process_uptime_seconds gauge
process_uptime_seconds 832.843322
# HELP glove_http_requests_total Total number of HTTP requests served.
# TYPE glove_http_requests_total counter
glove_http_requests_total 23
# HELP glove_http_errors_total Total number of HTTP error responses (4xx + 5xx).
# TYPE glove_http_errors_total counter
glove_http_errors_total 0
```

## Prometheus Integration

Add Glove to your Prometheus `scrape_configs`:

```yaml
scrape_configs:
  - job_name: "glove"
    scrape_interval: 15s
    static_configs:
      - targets: ["localhost:8080"]
    metrics_path: "/api/metrics"
```

```admonish note
`/api/metrics` is subject to the per-IP rate limit like the other API endpoints; a 15 s scrape interval stays well within it.
```

## Frontend Metrics Panel

The frontend includes a built-in metrics view accessible from the navigation rail. It polls `/api/metrics` and displays CPU time, resident and virtual memory, open file descriptors, threads, uptime, and the HTTP request and error counters.
