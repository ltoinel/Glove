# Metrics

```
GET /api/metrics
```

Returns server metrics in Prometheus text format.

## Available Metrics

### Process Metrics
- `process_cpu_usage` — Current CPU usage percentage
- `process_memory_bytes` — Resident memory in bytes
- `process_uptime_seconds` — Time since server start

### HTTP Metrics
- `http_requests_total` — Total number of HTTP requests received
- `http_errors_total` — Total number of HTTP error responses (4xx, 5xx)

### GTFS Metrics
- `gtfs_agencies` — Number of loaded agencies
- `gtfs_routes` — Number of loaded routes
- `gtfs_stops` — Number of loaded stops
- `gtfs_trips` — Number of loaded trips
- `gtfs_stop_times` — Number of loaded stop times

## Example

```bash
curl http://localhost:8080/api/metrics
```

```
# HELP process_cpu_usage Current CPU usage
# TYPE process_cpu_usage gauge
process_cpu_usage 12.5
# HELP process_memory_bytes Resident memory
# TYPE process_memory_bytes gauge
process_memory_bytes 524288000
# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total 15423
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

## Frontend Metrics Panel

The frontend includes a built-in metrics dashboard accessible from the sidebar. It displays live values for CPU, memory, uptime, request counts, and GTFS statistics, polling the `/api/metrics` and `/api/status` endpoints.
