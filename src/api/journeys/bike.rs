//! Bicycle journey endpoint via Valhalla routing engine.
//!
//! Calls Valhalla's `/route` API with `"bicycle"` costing to compute
//! cycling directions between two geographic coordinates.
//! Returns two journey variants: standard bike and e-bike.

use actix_web::{HttpResponse, get, web};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::valhalla::{DirectionsOptions, Location, RawManeuver, RouteRequest, RouteResponse};
use crate::config::AppConfig;
use crate::util::parse_from_to;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/journeys/bike`.
///
/// Coordinates are passed as `lon;lat` strings (same convention as Navitia).
#[derive(Debug, Deserialize, IntoParams)]
pub struct BikeQuery {
    /// Origin as `lon;lat`.
    pub from: String,
    /// Destination as `lon;lat`.
    pub to: String,
    /// Include turn-by-turn maneuvers in the response (default: false).
    pub maneuvers: Option<bool>,
    /// Language for maneuver instructions (e.g. "fr-FR", "en-US").
    pub language: Option<String>,
}

// ---------------------------------------------------------------------------
// Elevation via Valhalla /height API
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HeightRequest {
    shape: Vec<Location>,
}

#[derive(Debug, Deserialize)]
struct HeightResponse {
    #[serde(default)]
    height: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level response for `GET /api/journeys/bike`.
#[derive(Debug, Serialize, ToSchema)]
pub struct BikeResponse {
    pub journeys: Vec<BikeJourney>,
}

/// A cycling journey from origin to destination.
#[derive(Debug, Serialize, ToSchema)]
pub struct BikeJourney {
    /// Bike profile: "city", "ebike", or "road".
    #[serde(rename = "type")]
    pub bike_type: String,
    /// Total duration in seconds.
    pub duration: u32,
    /// Total distance in meters.
    pub distance: u32,
    /// Elevation gain in meters.
    pub elevation_gain: u32,
    /// Elevation loss in meters.
    pub elevation_loss: u32,
    /// Encoded polyline shape of the route.
    pub shape: String,
    /// Elevation values (meters) sampled along the route.
    pub heights: Vec<f64>,
    /// Turn-by-turn maneuvers (only included when requested).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maneuvers: Option<Vec<Maneuver>>,
}

/// A single maneuver in a cycling journey.
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Decode a Valhalla encoded polyline (precision 6) into a list of (lat, lon).
fn decode_polyline(encoded: &str) -> Vec<(f64, f64)> {
    let mut coords = Vec::new();
    let mut lat: i64 = 0;
    let mut lon: i64 = 0;
    let mut i = 0;
    let bytes = encoded.as_bytes();

    while i < bytes.len() {
        let mut shift = 0;
        let mut result: i64 = 0;
        loop {
            let b = (bytes[i] as i64) - 63;
            i += 1;
            result |= (b & 0x1f) << shift;
            shift += 5;
            if b < 0x20 {
                break;
            }
        }
        lat += if result & 1 != 0 {
            !(result >> 1)
        } else {
            result >> 1
        };

        shift = 0;
        result = 0;
        loop {
            let b = (bytes[i] as i64) - 63;
            i += 1;
            result |= (b & 0x1f) << shift;
            shift += 5;
            if b < 0x20 {
                break;
            }
        }
        lon += if result & 1 != 0 {
            !(result >> 1)
        } else {
            result >> 1
        };

        coords.push((lat as f64 / 1e6, lon as f64 / 1e6));
    }

    coords
}

/// Compute elevation gain and loss from a list of elevation values.
fn compute_elevation(heights: &[f64]) -> (u32, u32) {
    let mut gain: f64 = 0.0;
    let mut loss: f64 = 0.0;
    for pair in heights.windows(2) {
        let diff = pair[1] - pair[0];
        if diff > 0.0 {
            gain += diff;
        } else {
            loss -= diff;
        }
    }
    (gain.round() as u32, loss.round() as u32)
}

/// Build Valhalla bicycle costing_options from a bike profile.
fn bike_costing_options(profile: &crate::config::BikeProfile) -> serde_json::Value {
    serde_json::json!({
        "bicycle": {
            "cycling_speed": profile.cycling_speed,
            "use_roads": profile.use_roads,
            "use_hills": profile.use_hills,
            "bicycle_type": profile.bicycle_type
        }
    })
}

/// Bike profile definitions.
const BIKE_PROFILES: &[&str] = &["city", "ebike", "road"];

/// Maximum number of sampled points sent to Valhalla's /height endpoint.
const ELEVATION_SAMPLE_LIMIT: usize = 200;

/// Fetch elevation data from Valhalla's /height endpoint.
async fn fetch_elevation(
    client: &reqwest::Client,
    valhalla_base: &str,
    coords: &[(f64, f64)],
) -> Vec<f64> {
    let step = if coords.len() > ELEVATION_SAMPLE_LIMIT {
        coords.len() / ELEVATION_SAMPLE_LIMIT
    } else {
        1
    };
    let sampled: Vec<Location> = coords
        .iter()
        .step_by(step)
        .map(|(lat, lon)| Location {
            lat: *lat,
            lon: *lon,
        })
        .collect();

    let height_url = format!("{}/height", valhalla_base);
    let req = HeightRequest { shape: sampled };

    match client.post(&height_url).json(&req).send().await {
        Ok(resp) if resp.status().is_success() => resp
            .json::<HeightResponse>()
            .await
            .map(|h| h.height)
            .unwrap_or_default(),
        Ok(resp) => {
            tracing::debug!("Valhalla /height returned {}", resp.status());
            Vec::new()
        }
        Err(e) => {
            tracing::debug!("Valhalla /height unreachable: {e}");
            Vec::new()
        }
    }
}

/// Convert raw Valhalla maneuvers to bike maneuvers.
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

/// Compute cycling journeys between two coordinates via Valhalla.
///
/// Returns three variants: city bike, e-bike, and road bike — each with its
/// own Valhalla routing profile (speed, road/hill preferences, bicycle type).
#[utoipa::path(
    get,
    path = "/api/journeys/bike",
    params(BikeQuery),
    responses(
        (status = 200, description = "Cycling journeys (city, e-bike, road)", body = BikeResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 502, description = "Valhalla routing engine error"),
    ),
    tag = "Journeys"
)]
#[get("/api/journeys/bike")]
pub async fn get_bike(query: web::Query<BikeQuery>, config: web::Data<AppConfig>) -> HttpResponse {
    let (from_lon, from_lat, to_lon, to_lat) = match parse_from_to(&query.from, &query.to) {
        Ok(c) => c,
        Err(e) => return e,
    };

    let valhalla_base = format!("http://{}:{}", config.valhalla.host, config.valhalla.port);
    let valhalla_url = format!("{}/route", valhalla_base);

    let locations = vec![
        Location {
            lat: from_lat,
            lon: from_lon,
        },
        Location {
            lat: to_lat,
            lon: to_lon,
        },
    ];

    let client = reqwest::Client::new();
    let include_maneuvers = query.maneuvers.unwrap_or(false);

    let profiles = [
        (&config.bike.city, BIKE_PROFILES[0]),
        (&config.bike.ebike, BIKE_PROFILES[1]),
        (&config.bike.road, BIKE_PROFILES[2]),
    ];

    let mut journeys = Vec::with_capacity(profiles.len());

    for (profile, type_name) in &profiles {
        let req = RouteRequest {
            locations: locations.clone(),
            costing: "bicycle".to_string(),
            costing_options: Some(bike_costing_options(profile)),
            directions_options: DirectionsOptions {
                units: "kilometers".to_string(),
                language: query.language.clone(),
            },
        };

        let resp = client.post(&valhalla_url).json(&req).send().await;
        match process_response(resp, type_name, &client, &valhalla_base, include_maneuvers).await {
            Ok(j) => journeys.push(j),
            Err(e) => return e,
        }
    }

    HttpResponse::Ok().json(BikeResponse { journeys })
}

/// Process a Valhalla route response into a BikeJourney, or return an HTTP error.
async fn process_response(
    resp: Result<reqwest::Response, reqwest::Error>,
    bike_type: &str,
    client: &reqwest::Client,
    valhalla_base: &str,
    include_maneuvers: bool,
) -> Result<BikeJourney, HttpResponse> {
    let resp = resp.map_err(|e| {
        HttpResponse::BadGateway().json(serde_json::json!({
            "error": { "id": "valhalla_error", "message": format!("Failed to reach Valhalla: {e}") }
        }))
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(HttpResponse::BadGateway().json(serde_json::json!({
            "error": { "id": "valhalla_error", "message": format!("Valhalla returned {status}: {body}") }
        })));
    }

    let valhalla_resp: RouteResponse = resp.json().await.map_err(|e| {
        HttpResponse::BadGateway().json(serde_json::json!({
            "error": { "id": "valhalla_error", "message": format!("Invalid Valhalla response: {e}") }
        }))
    })?;

    let trip = &valhalla_resp.trip;
    let leg = trip.legs.first().ok_or_else(|| {
        HttpResponse::BadGateway().json(serde_json::json!({
            "error": { "id": "valhalla_error", "message": "Valhalla returned no route legs" }
        }))
    })?;

    let coords = decode_polyline(&leg.shape);
    let heights = fetch_elevation(client, valhalla_base, &coords).await;
    let (elevation_gain, elevation_loss) = compute_elevation(&heights);

    let maneuvers = if include_maneuvers {
        Some(convert_maneuvers(&leg.maneuvers))
    } else {
        None
    };

    Ok(BikeJourney {
        bike_type: bike_type.to_string(),
        duration: trip.summary.time as u32,
        distance: (trip.summary.length * 1000.0) as u32,
        elevation_gain,
        elevation_loss,
        shape: leg.shape.clone(),
        heights,
        maneuvers,
    })
}
