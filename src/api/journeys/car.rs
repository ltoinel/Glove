//! Car journey endpoint via Valhalla routing engine.
//!
//! Calls Valhalla's `/route` API with `"auto"` costing to compute
//! driving directions between two geographic coordinates.

use actix_web::{HttpResponse, get, web};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::valhalla::{DirectionsOptions, Location, RawManeuver, RouteRequest, RouteResponse};
use crate::config::AppConfig;
use crate::util::parse_from_to;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/journeys/car`.
///
/// Coordinates are passed as `lon;lat` strings.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CarQuery {
    /// Origin as `lon;lat`.
    pub from: String,
    /// Destination as `lon;lat`.
    pub to: String,
    /// Language for maneuver instructions (e.g. "fr-FR", "en-US").
    pub language: Option<String>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level response for `GET /api/journeys/car`.
#[derive(Debug, Serialize, ToSchema)]
pub struct CarResponse {
    pub journeys: Vec<CarJourney>,
}

/// A driving journey from origin to destination.
#[derive(Debug, Serialize, ToSchema)]
pub struct CarJourney {
    /// Total duration in seconds.
    pub duration: u32,
    /// Total distance in meters.
    pub distance: u32,
    /// Encoded polyline shape of the route.
    pub shape: String,
    /// Turn-by-turn maneuvers (only included when requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuvers: Option<Vec<Maneuver>>,
}

/// A single maneuver in a driving journey.
#[derive(Debug, Serialize, ToSchema)]
pub struct Maneuver {
    pub instruction: String,
    /// Valhalla maneuver type.
    #[serde(rename = "type")]
    pub maneuver_type: u32,
    /// Distance in meters.
    pub distance: u32,
    /// Duration in seconds.
    pub duration: u32,
    /// Index into the encoded shape where this maneuver begins.
    pub begin_shape_index: usize,
}

/// Convert raw Valhalla maneuvers to car maneuvers.
fn convert_maneuvers(raw: &[RawManeuver]) -> Vec<Maneuver> {
    raw.iter()
        .map(|m| Maneuver {
            instruction: m.instruction.clone(),
            maneuver_type: m.maneuver_type,
            distance: (m.length * 1000.0) as u32,
            duration: m.time as u32,
            begin_shape_index: m.begin_shape_index,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Compute a driving journey between two coordinates via Valhalla.
#[utoipa::path(
    get,
    path = "/api/journeys/car",
    params(CarQuery),
    responses(
        (status = 200, description = "Driving journey", body = CarResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 502, description = "Valhalla routing engine error"),
    ),
    tag = "Journeys"
)]
#[get("/api/journeys/car")]
pub async fn get_car(query: web::Query<CarQuery>, config: web::Data<AppConfig>) -> HttpResponse {
    let (from_lon, from_lat, to_lon, to_lat) = match parse_from_to(&query.from, &query.to) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let valhalla_url = format!(
        "http://{}:{}/route",
        config.valhalla.host, config.valhalla.port
    );

    let valhalla_req = RouteRequest {
        locations: vec![
            Location {
                lat: from_lat,
                lon: from_lon,
            },
            Location {
                lat: to_lat,
                lon: to_lon,
            },
        ],
        costing: "auto".to_string(),
        costing_options: None,
        directions_options: DirectionsOptions {
            units: "kilometers".to_string(),
            language: query.language.clone(),
        },
    };

    let client = reqwest::Client::new();
    let resp = match client.post(&valhalla_url).json(&valhalla_req).send().await {
        Ok(r) => r,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": { "id": "valhalla_error", "message": format!("Failed to reach Valhalla: {e}") }
            }));
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return HttpResponse::BadGateway().json(serde_json::json!({
            "error": { "id": "valhalla_error", "message": format!("Valhalla returned {status}: {body}") }
        }));
    }

    let valhalla_resp: RouteResponse = match resp.json().await {
        Ok(r) => r,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": { "id": "valhalla_error", "message": format!("Invalid Valhalla response: {e}") }
            }));
        }
    };

    let trip = &valhalla_resp.trip;
    let leg = match trip.legs.first() {
        Some(l) => l,
        None => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": { "id": "valhalla_error", "message": "Valhalla returned no route legs" }
            }));
        }
    };

    let include_maneuvers = config.routing.maneuvers;
    let maneuvers = if include_maneuvers {
        Some(convert_maneuvers(&leg.maneuvers))
    } else {
        None
    };

    let journey = CarJourney {
        duration: trip.summary.time as u32,
        distance: (trip.summary.length * 1000.0) as u32,
        shape: leg.shape.clone(),
        maneuvers,
    };

    HttpResponse::Ok().json(CarResponse {
        journeys: vec![journey],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_maneuvers_scales_units() {
        let raw = vec![
            RawManeuver {
                instruction: "Continue".into(),
                length: 2.0,
                time: 120.0,
                maneuver_type: 1,
                begin_shape_index: 0,
            },
            RawManeuver {
                instruction: "Turn left".into(),
                length: 0.05,
                time: 5.0,
                maneuver_type: 4,
                begin_shape_index: 10,
            },
        ];
        let out = convert_maneuvers(&raw);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].distance, 2000);
        assert_eq!(out[0].duration, 120);
        assert_eq!(out[1].distance, 50);
        assert_eq!(out[1].begin_shape_index, 10);
    }

    #[test]
    fn convert_maneuvers_empty() {
        assert!(convert_maneuvers(&[]).is_empty());
    }

    fn unreachable_config() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.valhalla.host = "127.0.0.1".into();
        cfg.valhalla.port = 1;
        cfg
    }

    #[actix_web::test]
    async fn get_car_rejects_bad_coords() {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(unreachable_config()))
                .service(get_car),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/car?from=bad&to=2.4;48.9")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_car_success_against_mock() {
        let base = super::super::valhalla::test_support::spawn_mock_valhalla();
        let rest = base.strip_prefix("http://").unwrap();
        let (host, port_str) = rest.split_once(':').unwrap();
        let mut cfg = AppConfig::default();
        cfg.valhalla.host = host.into();
        cfg.valhalla.port = port_str.parse().unwrap();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(cfg))
                .service(get_car),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/car?from=2.3;48.8&to=2.4;48.9&maneuvers=true")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert!(body["journeys"][0]["duration"].as_u64().unwrap() > 0);
    }

    #[actix_web::test]
    async fn get_car_unreachable_valhalla_returns_502() {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(unreachable_config()))
                .service(get_car),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/car?from=2.3;48.8&to=2.4;48.9&maneuvers=true")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 502);
    }
}
