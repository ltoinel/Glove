//! GTFS data validation and management endpoints.

use actix_web::{HttpResponse, get, post, web};
use arc_swap::ArcSwap;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::shared::config::AppConfig;
use crate::shared::util::constant_time_eq;
use crate::transit::gtfs::{self, GtfsData};
use crate::transit::raptor::RaptorData;
use crate::transit::validation::{self, Bounds, ValidationInput, ValidationReport};

/// Response for `POST /api/gtfs/reload`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReloadResponse {
    pub status: String,
    pub loaded_at: String,
    pub gtfs: super::status::GtfsStats,
    pub raptor: super::status::RaptorStats,
}

/// Response for `GET /api/gtfs/status`.
#[derive(Debug, Serialize, ToSchema)]
pub struct GtfsStatusResponse {
    /// Last GTFS load timestamp (RFC 3339).
    pub loaded_at: String,
    pub gtfs: super::status::GtfsStats,
    pub raptor: super::status::RaptorStats,
}

/// Return GTFS data statistics and the last load timestamp.
///
/// This is the GTFS-specific counterpart to `GET /api/status` (which only
/// reports engine health and map defaults).
#[utoipa::path(
    get,
    path = "/api/gtfs/status",
    responses(
        (status = 200, description = "GTFS data statistics", body = GtfsStatusResponse),
    ),
    tag = "GTFS"
)]
#[get("/api/gtfs/status")]
pub async fn get_gtfs_status(shared: web::Data<ArcSwap<RaptorData>>) -> HttpResponse {
    let raptor_data = shared.load();
    HttpResponse::Ok().json(super::status::gtfs_status_payload(&raptor_data.stats))
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

/// Validate GTFS data by loading it from disk and running all checks.
///
/// Reads the feed afresh rather than the served index, so it reflects the
/// files on disk — e.g. a new download not yet hot-reloaded.
#[utoipa::path(
    get,
    path = "/api/gtfs/validate",
    responses(
        (status = 200, description = "GTFS validation report: feed overview, every rule's outcome and the issues found", body = ValidationReport),
        (status = 500, description = "Failed to load GTFS data"),
    ),
    tag = "GTFS"
)]
#[get("/api/gtfs/validate")]
pub async fn get_validate(config: web::Data<AppConfig>) -> HttpResponse {
    let data_dir = config.data.gtfs_dir();
    let map = &config.map;
    let bounds = Bounds {
        sw_lat: map.bounds_sw_lat,
        sw_lon: map.bounds_sw_lon,
        ne_lat: map.bounds_ne_lat,
        ne_lon: map.bounds_ne_lon,
    };

    let result = web::block(move || {
        let data_path = std::path::Path::new(&data_dir);
        let gtfs = GtfsData::load(data_path).map_err(|e| e.to_string())?;
        let input = ValidationInput {
            today: chrono::Local::now().date_naive(),
            bounds: Some(bounds),
            location_types: load_location_types(data_path),
        };
        Ok::<_, String>(validation::validate(&gtfs, &input))
    })
    .await;

    match result {
        Ok(Ok(validation)) => HttpResponse::Ok().json(validation),
        Ok(Err(e)) => {
            tracing::error!("GTFS validation failed to load data: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "load_failed", "message": e }
            }))
        }
        Err(e) => {
            tracing::error!("GTFS validation task panicked: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "validation_panic", "message": "Internal error during validation" }
            }))
        }
    }
}

/// `location_type` per stop, or nothing: the rules that need it are then
/// skipped rather than failing the whole validation.
fn load_location_types(data_dir: &std::path::Path) -> rustc_hash::FxHashMap<String, u8> {
    gtfs::load_location_types(data_dir).unwrap_or_else(|e| {
        tracing::warn!("location_type not loaded, dependent rules skipped: {e}");
        Default::default()
    })
}

/// Hot-reload GTFS data without downtime.
///
/// Spawns the reload on a blocking thread pool via [`web::block`].
/// The old data continues serving requests until the new RAPTOR index
/// is atomically swapped in via [`ArcSwap::store`].
#[utoipa::path(
    post,
    path = "/api/gtfs/reload",
    responses(
        (status = 200, description = "GTFS data reloaded successfully", body = ReloadResponse),
        (status = 401, description = "Invalid or missing API key"),
        (status = 403, description = "Reload endpoint disabled (no api_key configured)"),
        (status = 500, description = "Reload failed"),
    ),
    security(("api_key" = [])),
    tag = "GTFS"
)]
#[post("/api/gtfs/reload")]
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
    if !constant_time_eq(provided_key, expected_key) {
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
        let gtfs = crate::transit::gtfs::GtfsData::load(data_path).map_err(|e| e.to_string())?;
        let fingerprint = crate::transit::gtfs::gtfs_fingerprint(data_path);
        let new_data = crate::transit::raptor::RaptorData::build(gtfs, transfer_time);
        if let Err(e) = new_data.save(cache_path, &fingerprint) {
            tracing::warn!("Failed to save RAPTOR cache: {e}");
        }
        Ok::<_, String>(Arc::new(new_data))
    })
    .await;

    match result {
        Ok(Ok(new_data)) => {
            let mut resp = super::status::gtfs_status_payload(&new_data.stats);
            resp["status"] = serde_json::json!("reloaded");
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
    use crate::transit::gtfs;
    use rustc_hash::FxHashMap;

    fn make_test_gtfs() -> GtfsData {
        let mut stops = FxHashMap::default();
        stops.insert(
            "S1".into(),
            gtfs::Stop {
                stop_id: "S1".into(),
                stop_name: "A".into(),
                stop_lon: 2.0,
                stop_lat: 48.0,
                parent_station: String::new(),
                wheelchair_boarding: 0,
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
                wheelchair_accessible: 0,
            },
        );
        let stop_times = vec![gtfs::StopTime {
            trip_id: "T1".into(),
            arrival_time: "08:00:00".into(),
            departure_time: "08:01:00".into(),
            stop_id: "S1".into(),
            stop_sequence: 0,
            ..Default::default()
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
        GtfsData {
            agencies: vec![],
            routes,
            stops,
            trips,
            stop_times,
            calendars,
            calendar_dates: vec![],
            transfers: vec![],
            pathways: vec![],
            ..Default::default()
        }
    }

    #[actix_web::test]
    async fn post_reload_disabled_when_api_key_empty() {
        let data = Arc::new(crate::transit::raptor::RaptorData::build(
            make_test_gtfs(),
            120,
        ));
        let cfg = AppConfig::default();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(data)))
                .app_data(web::Data::new(cfg))
                .service(post_reload),
        )
        .await;
        let req = actix_web::test::TestRequest::post()
            .uri("/api/gtfs/reload")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn gtfs_status_returns_stats() {
        let data = Arc::new(crate::transit::raptor::RaptorData::build(
            make_test_gtfs(),
            120,
        ));
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(data)))
                .service(get_gtfs_status),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/gtfs/status")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert!(body["gtfs"]["stops"].as_u64().unwrap() > 0);
        assert!(body["raptor"]["patterns"].as_u64().is_some());
        assert!(body["loaded_at"].is_string());
    }

    #[actix_web::test]
    async fn post_reload_unauthorized_when_wrong_key() {
        let data = Arc::new(crate::transit::raptor::RaptorData::build(
            make_test_gtfs(),
            120,
        ));
        let mut cfg = AppConfig::default();
        cfg.server.api_key = "secret".into();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(data)))
                .app_data(web::Data::new(cfg))
                .service(post_reload),
        )
        .await;
        let req = actix_web::test::TestRequest::post()
            .uri("/api/gtfs/reload")
            .insert_header(("X-Api-Key", "wrong"))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn post_reload_authorized_attempts_load() {
        let data = Arc::new(crate::transit::raptor::RaptorData::build(
            make_test_gtfs(),
            120,
        ));
        let mut cfg = AppConfig::default();
        cfg.server.api_key = "secret".into();
        // Point data dir at non-existent path → load fails → 500
        cfg.data.dir = "/nonexistent-test-data".into();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(data)))
                .app_data(web::Data::new(cfg))
                .service(post_reload),
        )
        .await;
        let req = actix_web::test::TestRequest::post()
            .uri("/api/gtfs/reload")
            .insert_header(("X-Api-Key", "secret"))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);
    }

    fn write_minimal_gtfs(gtfs_dir: &std::path::Path) {
        std::fs::create_dir_all(gtfs_dir).unwrap();
        std::fs::write(
            gtfs_dir.join("agency.txt"),
            "agency_id,agency_name,agency_url,agency_timezone\nA1,Test,https://e,Europe/Paris\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("routes.txt"),
            "route_id,agency_id,route_short_name,route_long_name,route_type,route_color,route_text_color\nR1,A1,1,Line 1,1,FFCD00,000000\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("stops.txt"),
            "stop_id,stop_name,stop_lon,stop_lat,parent_station,wheelchair_boarding\n\
             S1,StopA,2.347,48.858,,0\n\
             S2,StopB,2.395,48.848,,0\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("trips.txt"),
            "route_id,service_id,trip_id,trip_headsign,wheelchair_accessible\nR1,SVC1,T1,StopB,0\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("stop_times.txt"),
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
             T1,08:00:00,08:01:00,S1,0\n\
             T1,08:10:00,08:11:00,S2,1\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("calendar.txt"),
            "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n\
             SVC1,1,1,1,1,1,1,1,20260101,20261231\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("calendar_dates.txt"),
            "service_id,date,exception_type\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("transfers.txt"),
            "from_stop_id,to_stop_id,min_transfer_time\n",
        )
        .unwrap();
    }

    #[actix_web::test]
    async fn post_reload_success_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = AppConfig::default();
        cfg.data.dir = dir.path().to_string_lossy().into();
        cfg.server.api_key = "secret".into();
        write_minimal_gtfs(std::path::Path::new(&cfg.data.gtfs_dir()));

        let data = Arc::new(crate::transit::raptor::RaptorData::build(
            make_test_gtfs(),
            120,
        ));
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(data)))
                .app_data(web::Data::new(cfg))
                .service(post_reload),
        )
        .await;
        let req = actix_web::test::TestRequest::post()
            .uri("/api/gtfs/reload")
            .insert_header(("X-Api-Key", "secret"))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["status"], "reloaded");
        assert!(body["gtfs"]["stops"].as_u64().unwrap() >= 2);
    }

    #[actix_web::test]
    async fn get_validate_success_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = AppConfig::default();
        cfg.data.dir = dir.path().to_string_lossy().into();
        write_minimal_gtfs(std::path::Path::new(&cfg.data.gtfs_dir()));

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(cfg))
                .service(get_validate),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/gtfs/validate")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert!(body["summary"]["total_checks"].as_u64().unwrap() > 0);
    }

    #[actix_web::test]
    async fn get_validate_with_missing_data_dir_returns_500() {
        let mut cfg = AppConfig::default();
        cfg.data.dir = "/nonexistent-test-data".into();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(cfg))
                .service(get_validate),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/gtfs/validate")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);
    }
}
