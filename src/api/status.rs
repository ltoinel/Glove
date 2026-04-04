//! Engine status and GTFS hot-reload endpoints.

use actix_web::{HttpResponse, get, post, web};
use arc_swap::ArcSwap;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppConfig;
use crate::raptor::RaptorData;

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
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
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
        "map": {
            "center": [config.map_center_lat, config.map_center_lon],
            "zoom": config.map_zoom,
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
        (status = 500, description = "Reload failed"),
    ),
    tag = "Status"
)]
#[post("/api/reload")]
pub async fn post_reload(
    shared: web::Data<ArcSwap<RaptorData>>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    let data_dir = config.data_dir.clone();
    let raptor_dir = config.raptor_dir.clone();
    let transfer_time = config.default_transfer_time;

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
