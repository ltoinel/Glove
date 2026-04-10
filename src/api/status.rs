//! Engine status and GTFS hot-reload endpoints.

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwap;
use serde::Serialize;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AppConfig, gtfs, raptor};
    use rustc_hash::FxHashMap;
    use std::sync::Arc;

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
