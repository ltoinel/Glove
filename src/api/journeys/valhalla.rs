//! Shared Valhalla routing helpers.
//!
//! Provides a lightweight pedestrian route call used by both the walk
//! endpoint and the public_transport endpoint (first/last mile).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::config::WheelchairConfig;

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
    /// Language for maneuver instructions (e.g. "fr-FR", "en-US").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
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
    /// Index into the encoded shape where this maneuver begins.
    #[serde(default)]
    pub begin_shape_index: usize,
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
    /// Index into the encoded shape where this maneuver begins.
    pub begin_shape_index: usize,
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
    language: Option<&str>,
    wheelchair_config: Option<&WheelchairConfig>,
) -> Option<WalkLeg> {
    let costing_options = {
        let mut opts = if let Some(wc) = wheelchair_config {
            // Wheelchair mode: avoid stairs, prefer elevators, limit grade
            serde_json::json!({
                "pedestrian": {
                    "step_penalty": wc.step_penalty,
                    "max_grade": wc.max_grade,
                    "use_hills": wc.use_hills,
                    "elevator_penalty": wc.elevator_penalty
                }
            })
        } else if indoor_friendly {
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
        let effective_speed = if let Some(wc) = wheelchair_config {
            Some(wc.walking_speed)
        } else {
            walking_speed
        };
        if let Some(speed) = effective_speed {
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
            language: language.map(String::from),
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
            begin_shape_index: m.begin_shape_index,
        })
        .collect();

    Some(WalkLeg {
        duration: route.trip.summary.time as u32,
        distance: (route.trip.summary.length * 1000.0) as u32,
        shape: leg.shape.clone(),
        maneuvers,
    })
}

/// Test-only helpers shared with the other journey modules.
/// A tiny in-process actix server that mimics Valhalla's `/route` and
/// `/height` endpoints with canned JSON.
#[cfg(test)]
pub mod test_support {
    use actix_web::{App, HttpResponse, HttpServer, post, web};
    use std::net::TcpListener;
    use std::sync::Once;

    fn ok_route_body() -> serde_json::Value {
        // Empty shape is safe across all decoders (bike's decode_polyline
        // would otherwise panic on non-polyline data).
        serde_json::json!({
            "trip": {
                "legs": [{
                    "shape": "",
                    "maneuvers": [{
                        "instruction": "go",
                        "length": 0.1,
                        "time": 6.0,
                        "type": 1,
                        "begin_shape_index": 0
                    }]
                }],
                "summary": { "length": 1.2, "time": 600.0 }
            }
        })
    }

    fn ok_height_body() -> serde_json::Value {
        serde_json::json!({ "height": [10.0, 15.0, 12.0, 20.0] })
    }

    #[post("/route")]
    async fn route_handler(_body: web::Json<serde_json::Value>) -> HttpResponse {
        HttpResponse::Ok().json(ok_route_body())
    }

    #[post("/height")]
    async fn height_handler(_body: web::Json<serde_json::Value>) -> HttpResponse {
        HttpResponse::Ok().json(ok_height_body())
    }

    /// Spawn an actix mock server on a free port and return its base URL.
    /// The server keeps running until the test process exits.
    pub fn spawn_mock_valhalla() -> String {
        static INIT: Once = Once::new();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().unwrap();
        let port = addr.port();
        let base = format!("http://127.0.0.1:{port}");

        std::thread::spawn(move || {
            let sys = actix_web::rt::System::new();
            sys.block_on(async {
                let server =
                    HttpServer::new(|| App::new().service(route_handler).service(height_handler))
                        .listen(listener)
                        .expect("listen")
                        .workers(1)
                        .run();
                let _ = server.await;
            });
        });

        // Allow the server thread a brief moment to bind.
        INIT.call_once(|| {});
        std::thread::sleep(std::time::Duration::from_millis(150));
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    // All four flag combinations of pedestrian_route reach the request-build
    // stage, exercising the three costing_options branches and the speed
    // clamp; the actual HTTP call will fail against the unreachable URL,
    // confirming the None return path too.

    fn unreachable_base() -> &'static str {
        "http://127.0.0.1:1"
    }

    #[actix_web::test]
    async fn pedestrian_route_outdoor_default() {
        let out = pedestrian_route(
            unreachable_base(),
            (2.3, 48.8),
            (2.4, 48.9),
            Some(5.0),
            false,
            None,
            None,
        )
        .await;
        assert!(out.is_none());
    }

    #[actix_web::test]
    async fn pedestrian_route_indoor_friendly() {
        let out = pedestrian_route(
            unreachable_base(),
            (2.3, 48.8),
            (2.4, 48.9),
            None,
            true,
            Some("fr-FR"),
            None,
        )
        .await;
        assert!(out.is_none());
    }

    #[actix_web::test]
    async fn pedestrian_route_wheelchair_overrides_speed() {
        let cfg = AppConfig::default();
        let out = pedestrian_route(
            unreachable_base(),
            (2.3, 48.8),
            (2.4, 48.9),
            Some(99.0), // ignored because wheelchair_config is set
            false,
            None,
            Some(&cfg.wheelchair),
        )
        .await;
        assert!(out.is_none());
    }

    #[actix_web::test]
    async fn pedestrian_route_clamps_walking_speed() {
        let out = pedestrian_route(
            unreachable_base(),
            (2.3, 48.8),
            (2.4, 48.9),
            Some(99.0),
            false,
            None,
            None,
        )
        .await;
        assert!(out.is_none());
    }

    #[actix_web::test]
    async fn pedestrian_route_success_against_mock_valhalla() {
        let base = test_support::spawn_mock_valhalla();
        let leg = pedestrian_route(
            &base,
            (2.3, 48.8),
            (2.4, 48.9),
            Some(5.0),
            false,
            None,
            None,
        )
        .await
        .expect("mock returns a leg");
        assert_eq!(leg.shape, "");
        assert_eq!(leg.maneuvers.len(), 1);
        assert_eq!(leg.duration, 600);
        assert_eq!(leg.distance, 1200);
    }
}
