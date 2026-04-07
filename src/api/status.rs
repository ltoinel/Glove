//! Engine status and GTFS hot-reload endpoints.

use actix_web::{HttpResponse, get, post, web};
use arc_swap::ArcSwap;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppConfig;
use crate::raptor::RaptorData;

/// Check Valhalla connectivity by hitting its /status endpoint.
async fn check_valhalla(config: &AppConfig) -> bool {
    let url = format!(
        "http://{}:{}/status",
        config.valhalla.host, config.valhalla.port
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
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

/// Response for `GET /api/status`.
#[derive(Debug, Serialize, ToSchema)]
pub struct StatusResponse {
    pub status: String,
    pub loaded_at: String,
    pub gtfs: GtfsStats,
    pub raptor: RaptorStats,
}

/// Response for `POST /api/reload`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReloadResponse {
    pub status: String,
    pub loaded_at: String,
    pub gtfs: GtfsStats,
    pub raptor: RaptorStats,
}

/// Return engine status: GTFS data statistics and last load timestamp.
#[utoipa::path(
    get,
    path = "/api/status",
    responses(
        (status = 200, description = "Engine status and GTFS statistics", body = StatusResponse),
    ),
    tag = "Status"
)]
#[get("/api/status")]
pub async fn get_status(
    shared: web::Data<ArcSwap<RaptorData>>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    let raptor_data = shared.load();
    let s = &raptor_data.stats;
    let valhalla_ok = check_valhalla(&config).await;
    HttpResponse::Ok().json(serde_json::json!({
        "status": if valhalla_ok { "ok" } else { "degraded" },
        "loaded_at": s.loaded_at.to_rfc3339(),
        "dependencies": {
            "valhalla": if valhalla_ok { "ok" } else { "unreachable" },
        },
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
        "map": {
            "center": [config.map.center_lat, config.map.center_lon],
            "zoom": config.map.zoom,
            "bounds": [[config.map.bounds_sw_lat, config.map.bounds_sw_lon],
                        [config.map.bounds_ne_lat, config.map.bounds_ne_lon]],
        }
    }))
}

/// Hot-reload GTFS data without downtime.
///
/// Spawns the reload on a blocking thread pool via [`web::block`].
/// The old data continues serving requests until the new RAPTOR index
/// is atomically swapped in via [`ArcSwap::store`].
#[utoipa::path(
    post,
    path = "/api/reload",
    responses(
        (status = 200, description = "GTFS data reloaded successfully", body = ReloadResponse),
        (status = 401, description = "Invalid or missing API key"),
        (status = 403, description = "Reload endpoint disabled (no api_key configured)"),
        (status = 500, description = "Reload failed"),
    ),
    security(("api_key" = [])),
    tag = "Status"
)]
#[post("/api/reload")]
pub async fn post_reload(
    req: actix_web::HttpRequest,
    shared: web::Data<ArcSwap<RaptorData>>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    // --- API key authentication ---
    let expected_key = &config.server.api_key;
    if expected_key.is_empty() {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": { "id": "disabled", "message": "Reload endpoint is disabled (no api_key configured)" }
        }));
    }
    let provided_key = req
        .headers()
        .get("X-Api-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided_key != expected_key {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": { "id": "unauthorized", "message": "Invalid or missing X-Api-Key header" }
        }));
    }

    let data_dir = config.data.gtfs_dir();
    let raptor_dir = config.data.raptor_dir();
    let transfer_time = config.routing.default_transfer_time;

    let result = web::block(move || {
        let data_path = std::path::Path::new(&data_dir);
        let cache_path = std::path::Path::new(&raptor_dir);
        let gtfs = crate::gtfs::GtfsData::load(data_path).map_err(|e| e.to_string())?;
        let fingerprint = crate::gtfs::gtfs_fingerprint(data_path);
        let new_data = crate::raptor::RaptorData::build(gtfs, transfer_time);
        if let Err(e) = new_data.save(cache_path, &fingerprint) {
            tracing::warn!("Failed to save RAPTOR cache: {e}");
        }
        Ok::<_, String>(Arc::new(new_data))
    })
    .await;

    match result {
        Ok(Ok(new_data)) => {
            let s = &new_data.stats;
            let resp = serde_json::json!({
                "status": "reloaded",
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
                }
            });
            shared.store(new_data);
            tracing::info!("GTFS data reloaded");
            HttpResponse::Ok().json(resp)
        }
        Ok(Err(e)) => {
            tracing::error!("Reload failed: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "reload_failed", "message": e }
            }))
        }
        Err(e) => {
            tracing::error!("Reload task panicked: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "reload_panic", "message": "Internal error during reload" }
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AppConfig, gtfs, raptor};
    use rustc_hash::FxHashMap;

    fn make_test_raptor() -> Arc<RaptorData> {
        let mut stops = FxHashMap::default();
        stops.insert(
            "S1".into(),
            gtfs::Stop {
                stop_id: "S1".into(),
                stop_name: "A".into(),
                stop_lon: 2.0,
                stop_lat: 48.0,
                parent_station: String::new(),
            },
        );
        let mut routes = FxHashMap::default();
        routes.insert(
            "R1".into(),
            gtfs::Route {
                route_id: "R1".into(),
                agency_id: "A1".into(),
                route_short_name: "1".into(),
                route_long_name: "L".into(),
                route_type: 1,
                route_color: String::new(),
                route_text_color: String::new(),
            },
        );
        let mut trips = FxHashMap::default();
        trips.insert(
            "T1".into(),
            gtfs::Trip {
                route_id: "R1".into(),
                service_id: "SVC1".into(),
                trip_id: "T1".into(),
                trip_headsign: "A".into(),
            },
        );
        let stop_times = vec![gtfs::StopTime {
            trip_id: "T1".into(),
            arrival_time: "08:00:00".into(),
            departure_time: "08:01:00".into(),
            stop_id: "S1".into(),
            stop_sequence: 0,
        }];
        let mut calendars = FxHashMap::default();
        calendars.insert(
            "SVC1".into(),
            gtfs::Calendar {
                service_id: "SVC1".into(),
                monday: 1,
                tuesday: 1,
                wednesday: 1,
                thursday: 1,
                friday: 1,
                saturday: 1,
                sunday: 1,
                start_date: "20260101".into(),
                end_date: "20261231".into(),
            },
        );
        let gtfs_data = gtfs::GtfsData {
            agencies: vec![],
            routes,
            stops,
            trips,
            stop_times,
            calendars,
            calendar_dates: vec![],
            transfers: vec![],
            pathways: vec![],
        };
        Arc::new(raptor::RaptorData::build(gtfs_data, 120))
    }

    #[actix_web::test]
    async fn status_returns_ok() {
        let data = make_test_raptor();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(data)))
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
        assert!(body["gtfs"]["stops"].as_u64().unwrap() > 0);
        assert!(body["map"]["center"].is_array());
        assert!(body["map"]["bounds"].is_array());
    }
}
