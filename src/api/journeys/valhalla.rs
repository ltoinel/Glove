//! Shared Valhalla routing helpers.
//!
//! Provides a lightweight pedestrian route call used by both the walk
//! endpoint and the public_transport endpoint (first/last mile).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Valhalla request / response types
// ---------------------------------------------------------------------------

/// A geographic location for Valhalla requests.
#[derive(Clone, Serialize)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
}

/// Valhalla route request body.
#[derive(Serialize)]
pub struct RouteRequest {
    pub locations: Vec<Location>,
    pub costing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub costing_options: Option<serde_json::Value>,
    pub directions_options: DirectionsOptions,
}

/// Directions options within a Valhalla request.
#[derive(Serialize)]
pub struct DirectionsOptions {
    pub units: String,
}

/// Valhalla route response.
#[derive(Deserialize)]
pub struct RouteResponse {
    pub trip: Trip,
}

/// Trip within a Valhalla route response.
#[derive(Deserialize)]
pub struct Trip {
    pub legs: Vec<Leg>,
    pub summary: Summary,
}

/// A leg of a Valhalla route.
#[derive(Deserialize)]
pub struct Leg {
    pub shape: String,
    #[serde(default)]
    pub maneuvers: Vec<RawManeuver>,
}

/// A raw maneuver from Valhalla (before unit conversion).
#[derive(Deserialize)]
pub struct RawManeuver {
    pub instruction: String,
    /// Distance in kilometers (Valhalla unit).
    pub length: f64,
    /// Duration in seconds.
    pub time: f64,
    #[serde(rename = "type")]
    pub maneuver_type: u32,
}

/// Summary statistics for a Valhalla route.
#[derive(Deserialize)]
pub struct Summary {
    /// Total distance in kilometers.
    pub length: f64,
    /// Total duration in seconds.
    pub time: f64,
}

// ---------------------------------------------------------------------------
// Public result types
// ---------------------------------------------------------------------------

/// A single maneuver in a pedestrian route.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct WalkManeuver {
    pub instruction: String,
    #[serde(rename = "type")]
    pub maneuver_type: u32,
    /// Distance in meters.
    pub distance: u32,
    /// Duration in seconds.
    pub duration: u32,
}

/// Result of a pedestrian route computation.
#[derive(Clone)]
pub struct WalkLeg {
    /// Duration in seconds.
    pub duration: u32,
    /// Distance in meters.
    pub distance: u32,
    /// Encoded polyline (Valhalla precision-6).
    pub shape: String,
    /// Turn-by-turn maneuvers.
    pub maneuvers: Vec<WalkManeuver>,
}

// ---------------------------------------------------------------------------
// Pedestrian route helper
// ---------------------------------------------------------------------------

/// Compute a pedestrian route between two coordinates via Valhalla.
///
/// When `indoor_friendly` is true, step and elevator penalties are removed
/// to favor underground passages in stations. This is used for transfer
/// sections where stairs/escalators/elevators are the normal path.
///
/// Returns `None` if Valhalla is unreachable or returns an error.
pub async fn pedestrian_route(
    valhalla_base: &str,
    from: (f64, f64), // (lon, lat)
    to: (f64, f64),   // (lon, lat)
    walking_speed: Option<f64>,
    indoor_friendly: bool,
) -> Option<WalkLeg> {
    let costing_options = {
        let mut opts = if indoor_friendly {
            // For station transfers: no penalty for stairs/elevators/escalators
            // since they are the expected path through underground passages
            serde_json::json!({
                "pedestrian": {
                    "step_penalty": 0,
                    "elevator_penalty": 0,
                    "use_tunnels": 1.0
                }
            })
        } else {
            // For first/last mile: penalize stairs (user may have luggage)
            serde_json::json!({
                "pedestrian": {
                    "step_penalty": 30,
                    "elevator_penalty": 60
                }
            })
        };
        if let Some(speed) = walking_speed {
            opts["pedestrian"]["walking_speed"] = serde_json::json!(speed.clamp(0.5, 25.5));
        }
        Some(opts)
    };

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

    let maneuvers = leg
        .maneuvers
        .iter()
        .map(|m| WalkManeuver {
            instruction: m.instruction.clone(),
            maneuver_type: m.maneuver_type,
            distance: (m.length * 1000.0) as u32,
            duration: m.time as u32,
        })
        .collect();

    Some(WalkLeg {
        duration: route.trip.summary.time as u32,
        distance: (route.trip.summary.length * 1000.0) as u32,
        shape: leg.shape.clone(),
        maneuvers,
    })
}
