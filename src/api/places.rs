//! Stop and address name autocomplete endpoint.

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

use crate::ban::BanData;
use crate::raptor::RaptorData;

/// Query parameters for `GET /api/places`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PlacesQuery {
    /// Search query string (min 2 characters).
    pub q: Option<String>,
    /// Maximum number of results (default 10, max 50).
    pub limit: Option<usize>,
}

/// Response for `GET /api/places`.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlacesResponse {
    pub places: Vec<PlaceResult>,
}

/// A single place result from autocomplete search.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaceResult {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub place_type: String,
    pub coord: super::Coord,
}

/// Autocomplete search across GTFS stops and BAN addresses.
/// Returns up to `limit` matching results ranked by relevance.
#[utoipa::path(
    get,
    path = "/api/places",
    params(PlacesQuery),
    responses(
        (status = 200, description = "Matching stops and addresses", body = PlacesResponse),
    ),
    tag = "Places"
)]
#[get("/api/places")]
pub async fn get_places(
    query: web::Query<PlacesQuery>,
    shared: web::Data<ArcSwap<RaptorData>>,
    ban: web::Data<Arc<BanData>>,
) -> HttpResponse {
    let raptor_data = shared.load();
    let raw_q = query.q.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(10).min(50);

    // Strip leading/trailing digits and whitespace (e.g. "12 rue de Rivoli" → "rue de Rivoli")
    let q: String = raw_q
        .chars()
        .filter(|c| !c.is_ascii_digit())
        .collect::<String>();
    let q = q.trim();

    if q.len() < 2 {
        return HttpResponse::Ok().json(serde_json::json!({ "places": [] }));
    }

    // Search both sources
    let stop_results = raptor_data.search_stops(q, limit);
    let ban_results = ban.search(q, limit);

    // Stops first (higher priority), then addresses, up to limit
    let mut places: Vec<serde_json::Value> = Vec::with_capacity(limit);

    for (idx, name, _id) in &stop_results {
        if places.len() >= limit {
            break;
        }
        let stop = &raptor_data.stops[*idx];
        places.push(serde_json::json!({
            "id": stop.stop_id,
            "name": name,
            "type": "stop",
            "coord": {
                "lon": stop.stop_lon,
                "lat": stop.stop_lat,
            }
        }));
    }

    for entry in &ban_results {
        if places.len() >= limit {
            break;
        }
        // Use lon;lat as ID so resolve_stop() can find the nearest stop
        let id = format!("{};{}", entry.lon, entry.lat);
        places.push(serde_json::json!({
            "id": id,
            "name": entry.label,
            "type": "address",
            "coord": {
                "lon": entry.lon,
                "lat": entry.lat,
            }
        }));
    }

    HttpResponse::Ok().json(serde_json::json!({ "places": places }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ban, config, gtfs, raptor};
    use std::collections::HashMap;

    fn make_test_data() -> (Arc<RaptorData>, Arc<BanData>) {
        let mut stops = HashMap::new();
        stops.insert(
            "S1".into(),
            gtfs::Stop {
                stop_id: "S1".into(),
                stop_name: "Châtelet".into(),
                stop_lon: 2.347,
                stop_lat: 48.858,
                parent_station: String::new(),
            },
        );
        stops.insert(
            "S2".into(),
            gtfs::Stop {
                stop_id: "S2".into(),
                stop_name: "Nation".into(),
                stop_lon: 2.395,
                stop_lat: 48.848,
                parent_station: String::new(),
            },
        );
        let mut trips = HashMap::new();
        trips.insert(
            "T1".into(),
            gtfs::Trip {
                route_id: "R1".into(),
                service_id: "SVC1".into(),
                trip_id: "T1".into(),
                trip_headsign: "Nation".into(),
            },
        );
        let stop_times = vec![
            gtfs::StopTime {
                trip_id: "T1".into(),
                arrival_time: "08:00:00".into(),
                departure_time: "08:01:00".into(),
                stop_id: "S1".into(),
                stop_sequence: 0,
            },
            gtfs::StopTime {
                trip_id: "T1".into(),
                arrival_time: "08:10:00".into(),
                departure_time: "08:11:00".into(),
                stop_id: "S2".into(),
                stop_sequence: 1,
            },
        ];
        let mut calendars = HashMap::new();
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
        let mut routes = HashMap::new();
        routes.insert(
            "R1".into(),
            gtfs::Route {
                route_id: "R1".into(),
                agency_id: "A1".into(),
                route_short_name: "1".into(),
                route_long_name: "Line 1".into(),
                route_type: 1,
                route_color: String::new(),
                route_text_color: String::new(),
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
        };
        let raptor_data = Arc::new(raptor::RaptorData::build(gtfs_data, 120));
        let ban_data = Arc::new(ban::BanData {
            entries: vec![ban::BanEntry {
                label: "Rue de Rivoli, 75001 Paris".into(),
                name_lower: crate::text::normalize("Rue de Rivoli, 75001 Paris"),
                lon: 2.3387,
                lat: 48.8606,
            }],
        });
        (raptor_data, ban_data)
    }

    #[actix_web::test]
    async fn places_short_query() {
        let (raptor, ban) = make_test_data();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(raptor)))
                .app_data(web::Data::new(ban))
                .service(get_places),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/places?q=a")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert!(body["places"].as_array().unwrap().is_empty());
    }

    #[actix_web::test]
    async fn places_returns_stops_first() {
        let (raptor, ban) = make_test_data();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(raptor)))
                .app_data(web::Data::new(ban))
                .service(get_places),
        )
        .await;
        // "at" matches "Châtelet" (stop) and "Rivoli" won't match
        let req = actix_web::test::TestRequest::get()
            .uri("/api/places?q=chatelet")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        let places = body["places"].as_array().unwrap();
        assert!(!places.is_empty());
        assert_eq!(places[0]["type"], "stop");
    }

    #[actix_web::test]
    async fn places_strips_numbers() {
        let (raptor, ban) = make_test_data();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(raptor)))
                .app_data(web::Data::new(ban))
                .service(get_places),
        )
        .await;
        // "12 nation" → strips "12" → searches "nation"
        let req = actix_web::test::TestRequest::get()
            .uri("/api/places?q=12%20nation")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        let places = body["places"].as_array().unwrap();
        assert!(!places.is_empty());
    }

    #[actix_web::test]
    async fn places_returns_addresses() {
        let (raptor, ban) = make_test_data();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(raptor)))
                .app_data(web::Data::new(ban))
                .service(get_places),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/places?q=rivoli")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        let places = body["places"].as_array().unwrap();
        assert!(places.iter().any(|p| p["type"] == "address"));
    }

    #[actix_web::test]
    async fn places_no_query() {
        let (raptor, ban) = make_test_data();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(raptor)))
                .app_data(web::Data::new(ban))
                .service(get_places),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/places")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }
}
