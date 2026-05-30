//! Engine status and GTFS hot-reload endpoints.

use actix_web::{HttpResponse, get, web};
use serde::Serialize;
use utoipa::ToSchema;

use crate::config::AppConfig;

/// Check Valhalla connectivity by hitting its /status endpoint.
///
/// The URL is built exclusively from admin-controlled config values
/// (`valhalla.host` and `valhalla.port`), not from user input.
async fn check_valhalla(config: &AppConfig) -> bool {
    // Validate host: only allow hostnames and IPs, no scheme or slashes
    let host = &config.valhalla.host;
    if host.is_empty() || host.contains('/') || host.contains(':') {
        tracing::warn!("Invalid Valhalla host in config: {host}");
        return false;
    }
    let url = format!("http://{}:{}/status", host, config.valhalla.port);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build();
    match client {
        Ok(c) => c
            .get(&url)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success()),
        Err(_) => false,
    }
}

/// GTFS data statistics.
#[derive(Debug, Serialize, ToSchema)]
pub struct GtfsStats {
    pub agencies: usize,
    pub routes: usize,
    pub stops: usize,
    pub trips: usize,
    pub stop_times: usize,
    pub calendars: usize,
    pub calendar_dates: usize,
    pub transfers: usize,
}

/// RAPTOR index statistics.
#[derive(Debug, Serialize, ToSchema)]
pub struct RaptorStats {
    pub patterns: usize,
    pub services: usize,
}

/// Dependency health.
#[derive(Debug, Serialize, ToSchema)]
pub struct Dependencies {
    /// `"ok"` or `"unreachable"`.
    pub valhalla: String,
}

/// Map display defaults (from config, not GTFS).
#[derive(Debug, Serialize, ToSchema)]
pub struct MapInfo {
    pub center: [f64; 2],
    pub zoom: u8,
    pub bounds: [[f64; 2]; 2],
}

/// Response for `GET /api/status` — engine health and map defaults only.
/// GTFS statistics live at `GET /api/gtfs/status`.
#[derive(Debug, Serialize, ToSchema)]
pub struct StatusResponse {
    /// `"ok"` when all dependencies are healthy, else `"degraded"`.
    pub status: String,
    pub dependencies: Dependencies,
    pub map: MapInfo,
}

/// Build the GTFS + RAPTOR statistics payload, shared by `GET /api/gtfs/status`
/// and `POST /api/gtfs/reload`.
pub fn gtfs_status_payload(s: &crate::raptor::GtfsStats) -> serde_json::Value {
    serde_json::json!({
        "loaded_at": s.loaded_at.to_rfc3339(),
        "gtfs": {
            "agencies": s.agencies,
            "routes": s.routes,
            "stops": s.stops,
            "trips": s.trips,
            "stop_times": s.stop_times,
            "calendars": s.calendars,
            "calendar_dates": s.calendar_dates,
            "transfers": s.transfers,
        },
        "raptor": {
            "patterns": s.patterns,
            "services": s.services,
        },
    })
}

/// Return engine health and map defaults. GTFS data is intentionally **not**
/// included here — see `GET /api/gtfs/status`.
#[utoipa::path(
    get,
    path = "/api/status",
    responses(
        (status = 200, description = "Engine health and map defaults", body = StatusResponse),
    ),
    tag = "Status"
)]
#[get("/api/status")]
pub async fn get_status(config: web::Data<AppConfig>) -> HttpResponse {
    let valhalla_ok = check_valhalla(&config).await;
    HttpResponse::Ok().json(serde_json::json!({
        "status": if valhalla_ok { "ok" } else { "degraded" },
        "dependencies": {
            "valhalla": if valhalla_ok { "ok" } else { "unreachable" },
        },
        "map": {
            "center": [config.map.center_lat, config.map.center_lon],
            "zoom": config.map.zoom,
            "bounds": [[config.map.bounds_sw_lat, config.map.bounds_sw_lon],
                        [config.map.bounds_ne_lat, config.map.bounds_ne_lon]],
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[actix_web::test]
    async fn check_valhalla_rejects_empty_host() {
        let mut cfg = AppConfig::default();
        cfg.valhalla.host = String::new();
        assert!(!check_valhalla(&cfg).await);
    }

    #[actix_web::test]
    async fn check_valhalla_rejects_host_with_slash_or_scheme() {
        let mut cfg = AppConfig::default();
        cfg.valhalla.host = "http://evil".into();
        assert!(!check_valhalla(&cfg).await);
        cfg.valhalla.host = "host/path".into();
        assert!(!check_valhalla(&cfg).await);
    }

    #[actix_web::test]
    async fn check_valhalla_unreachable_returns_false() {
        let mut cfg = AppConfig::default();
        cfg.valhalla.host = "127.0.0.1".into();
        cfg.valhalla.port = 1; // unreachable
        assert!(!check_valhalla(&cfg).await);
    }

    #[actix_web::test]
    async fn status_returns_health_and_map_only() {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(AppConfig::default()))
                .service(get_status),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/status")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        // Status is "degraded" when Valhalla is not running (test environment)
        let status = body["status"].as_str().unwrap();
        assert!(status == "ok" || status == "degraded");
        assert!(body["dependencies"]["valhalla"].is_string());
        assert!(body["map"]["center"].is_array());
        assert!(body["map"]["bounds"].is_array());
        // GTFS data must NOT be in /api/status anymore.
        assert!(body["gtfs"].is_null());
        assert!(body["raptor"].is_null());
    }
}
