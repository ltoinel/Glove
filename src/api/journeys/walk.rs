//! Pedestrian journey endpoint via Valhalla routing engine.
//!
//! Calls Valhalla's `/route` API with `"pedestrian"` costing to compute
//! walking directions between two geographic coordinates.

use actix_web::{HttpResponse, get, web};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::config::AppConfig;

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
}

// ---------------------------------------------------------------------------
// Valhalla request / response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ValhallaLocation {
    lat: f64,
    lon: f64,
}

#[derive(Serialize)]
struct ValhallaRequest {
    locations: Vec<ValhallaLocation>,
    costing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    costing_options: Option<serde_json::Value>,
    directions_options: ValhallaDirectionsOptions,
}

#[derive(Serialize)]
struct ValhallaDirectionsOptions {
    units: String,
}

#[derive(Debug, Deserialize)]
struct ValhallaResponse {
    trip: ValhallaTrip,
}

#[derive(Debug, Deserialize)]
struct ValhallaTrip {
    legs: Vec<ValhallaLeg>,
    summary: ValhallaSummary,
}

#[derive(Debug, Deserialize)]
struct ValhallaLeg {
    shape: String,
    maneuvers: Vec<ValhallaManeuver>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ValhallaManeuver {
    instruction: String,
    length: f64,
    time: f64,
    #[serde(rename = "type")]
    maneuver_type: u32,
}

#[derive(Debug, Deserialize)]
struct ValhallaSummary {
    length: f64,
    time: f64,
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
    /// Turn-by-turn maneuvers.
    pub maneuvers: Vec<Maneuver>,
}

/// A single maneuver in a walking journey.
#[derive(Debug, Serialize, ToSchema)]
pub struct Maneuver {
    pub instruction: String,
    /// Distance in meters.
    pub distance: u32,
    /// Duration in seconds.
    pub duration: u32,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a `"lon;lat"` string into `(lon, lat)`.
fn parse_coord(s: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = s.split(';').collect();
    if parts.len() == 2 {
        let lon = parts[0].parse::<f64>().ok()?;
        let lat = parts[1].parse::<f64>().ok()?;
        Some((lon, lat))
    } else {
        None
    }
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
    let (from_lon, from_lat) = match parse_coord(&query.from) {
        Some(c) => c,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": { "id": "bad_request", "message": "'from' must be in 'lon;lat' format" }
            }));
        }
    };

    let (to_lon, to_lat) = match parse_coord(&query.to) {
        Some(c) => c,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": { "id": "bad_request", "message": "'to' must be in 'lon;lat' format" }
            }));
        }
    };

    let valhalla_url = format!(
        "http://{}:{}/route",
        config.valhalla.host, config.valhalla.port
    );

    // Build costing_options with walking speed and station navigation penalties
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

    let valhalla_req = ValhallaRequest {
        locations: vec![
            ValhallaLocation {
                lat: from_lat,
                lon: from_lon,
            },
            ValhallaLocation {
                lat: to_lat,
                lon: to_lon,
            },
        ],
        costing: "pedestrian".to_string(),
        costing_options,
        directions_options: ValhallaDirectionsOptions {
            units: "kilometers".to_string(),
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

    let valhalla_resp: ValhallaResponse = match resp.json().await {
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

    let maneuvers: Vec<Maneuver> = leg
        .maneuvers
        .iter()
        .map(|m| Maneuver {
            instruction: m.instruction.clone(),
            distance: (m.length * 1000.0) as u32,
            duration: m.time as u32,
        })
        .collect();

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
