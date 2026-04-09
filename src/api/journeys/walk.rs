//! Pedestrian journey endpoint via Valhalla routing engine.
//!
//! Calls Valhalla's `/route` API with `"pedestrian"` costing to compute
//! walking directions between two geographic coordinates.

use actix_web::{HttpResponse, get, web};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::config::AppConfig;
use crate::util::parse_from_to;
use super::valhalla::{DirectionsOptions, Location, RouteRequest, RouteResponse};

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/journeys/walk`.
///
/// Coordinates are passed as `lon;lat` strings (same convention as Navitia).
#[derive(Debug, Deserialize, IntoParams)]
pub struct WalkQuery {
    /// Origin as `lon;lat`.
    pub from: String,
    /// Destination as `lon;lat`.
    pub to: String,
    /// Walking speed in km/h (Valhalla range: 0.5–25.5, default ≈ 5.1).
    pub walking_speed: Option<f64>,
    /// Include turn-by-turn maneuvers in the response (default: false).
    pub maneuvers: Option<bool>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level response for `GET /api/journeys/walk`.
#[derive(Debug, Serialize, ToSchema)]
pub struct WalkResponse {
    pub journeys: Vec<WalkJourney>,
}

/// A walking journey from origin to destination.
#[derive(Debug, Serialize, ToSchema)]
pub struct WalkJourney {
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

/// A single maneuver in a walking journey.
#[derive(Debug, Serialize, ToSchema)]
pub struct Maneuver {
    pub instruction: String,
    /// Valhalla maneuver type (e.g. 39=elevator, 40=stairs, 41=escalator, 42=enter building, 43=exit building).
    #[serde(rename = "type")]
    pub maneuver_type: u32,
    /// Distance in meters.
    pub distance: u32,
    /// Duration in seconds.
    pub duration: u32,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Compute a pedestrian journey between two coordinates via Valhalla.
#[utoipa::path(
    get,
    path = "/api/journeys/walk",
    params(WalkQuery),
    responses(
        (status = 200, description = "Walking journey", body = WalkResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 502, description = "Valhalla routing engine error"),
    ),
    tag = "Journeys"
)]
#[get("/api/journeys/walk")]
pub async fn get_walk(query: web::Query<WalkQuery>, config: web::Data<AppConfig>) -> HttpResponse {
    let (from_lon, from_lat, to_lon, to_lat) = match parse_from_to(&query.from, &query.to) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let valhalla_url = format!(
        "http://{}:{}/route",
        config.valhalla.host, config.valhalla.port
    );

    let costing_options = {
        let mut opts = serde_json::json!({
            "pedestrian": {
                "step_penalty": 30,
                "elevator_penalty": 60
            }
        });
        if let Some(speed) = query.walking_speed {
            opts["pedestrian"]["walking_speed"] = serde_json::json!(speed.clamp(0.5, 25.5));
        }
        Some(opts)
    };

    let valhalla_req = RouteRequest {
        locations: vec![
            Location { lat: from_lat, lon: from_lon },
            Location { lat: to_lat, lon: to_lon },
        ],
        costing: "pedestrian".to_string(),
        costing_options,
        directions_options: DirectionsOptions { units: "kilometers".to_string() },
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

    let include_maneuvers = query.maneuvers.unwrap_or(false);
    let maneuvers = if include_maneuvers {
        Some(
            leg.maneuvers
                .iter()
                .map(|m| Maneuver {
                    instruction: m.instruction.clone(),
                    maneuver_type: m.maneuver_type,
                    distance: (m.length * 1000.0) as u32,
                    duration: m.time as u32,
                })
                .collect(),
        )
    } else {
        None
    };

    let journey = WalkJourney {
        duration: trip.summary.time as u32,
        distance: (trip.summary.length * 1000.0) as u32,
        shape: leg.shape.clone(),
        maneuvers,
    };

    HttpResponse::Ok().json(WalkResponse {
        journeys: vec![journey],
    })
}
