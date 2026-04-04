//! Engine status and GTFS hot-reload endpoints.

use actix_web::{HttpResponse, get, post, web};
use arc_swap::ArcSwap;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::raptor::RaptorData;

/// Return engine status: GTFS data statistics and last load timestamp.
#[get("/api/status")]
pub async fn get_status(shared: web::Data<ArcSwap<RaptorData>>) -> HttpResponse {
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
        }
    }))
}

/// Hot-reload GTFS data without downtime.
///
/// Spawns the reload on a blocking thread pool via [`web::block`].
/// The old data continues serving requests until the new RAPTOR index
/// is atomically swapped in via [`ArcSwap::store`].
#[post("/api/reload")]
pub async fn post_reload(
    shared: web::Data<ArcSwap<RaptorData>>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    let data_dir = config.data_dir.clone();
    let transfer_time = config.default_transfer_time;

    let result = web::block(move || {
        let gtfs = crate::gtfs::GtfsData::load(std::path::Path::new(&data_dir))
            .map_err(|e| e.to_string())?;
        let new_data = crate::raptor::RaptorData::build(gtfs, transfer_time);
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
