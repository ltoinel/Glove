//! Shared Valhalla routing helpers.
//!
//! Provides a lightweight pedestrian route call used by both the walk
//! endpoint and the public_transport endpoint (first/last mile).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Valhalla request / response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize)]
struct RouteRequest {
    locations: Vec<Location>,
    costing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    costing_options: Option<serde_json::Value>,
    directions_options: DirectionsOptions,
}

#[derive(Serialize)]
struct DirectionsOptions {
    units: String,
}

#[derive(Deserialize)]
struct RouteResponse {
    trip: Trip,
}

#[derive(Deserialize)]
struct Trip {
    legs: Vec<Leg>,
    summary: Summary,
}

#[derive(Deserialize)]
struct Leg {
    shape: String,
}

#[derive(Deserialize)]
struct Summary {
    length: f64,
    time: f64,
}

// ---------------------------------------------------------------------------
// Public result type
// ---------------------------------------------------------------------------

/// Result of a pedestrian route computation.
#[derive(Clone)]
pub struct WalkLeg {
    /// Duration in seconds.
    pub duration: u32,
    /// Distance in meters.
    pub distance: u32,
    /// Encoded polyline (Valhalla precision-6).
    pub shape: String,
}

// ---------------------------------------------------------------------------
// Pedestrian route helper
// ---------------------------------------------------------------------------

/// Compute a pedestrian route between two coordinates via Valhalla.
///
/// Returns `None` if Valhalla is unreachable or returns an error.
pub async fn pedestrian_route(
    valhalla_base: &str,
    from: (f64, f64), // (lon, lat)
    to: (f64, f64),   // (lon, lat)
    walking_speed: Option<f64>,
) -> Option<WalkLeg> {
    let costing_options = walking_speed.map(|speed| {
        let clamped = speed.clamp(0.5, 25.5);
        serde_json::json!({ "pedestrian": { "walking_speed": clamped } })
    });

    let req = RouteRequest {
        locations: vec![
            Location {
                lat: from.1,
                lon: from.0,
            },
            Location {
                lat: to.1,
                lon: to.0,
            },
        ],
        costing: "pedestrian".to_string(),
        costing_options,
        directions_options: DirectionsOptions {
            units: "kilometers".to_string(),
        },
    };

    let url = format!("{valhalla_base}/route");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    let resp = client.post(&url).json(&req).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let route: RouteResponse = resp.json().await.ok()?;
    let leg = route.trip.legs.first()?;

    Some(WalkLeg {
        duration: route.trip.summary.time as u32,
        distance: (route.trip.summary.length * 1000.0) as u32,
        shape: leg.shape.clone(),
    })
}
