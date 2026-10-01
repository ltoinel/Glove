//! Shared Valhalla routing helpers.
//!
//! Provides a lightweight pedestrian route call used by both the walk
//! endpoint and the public_transport endpoint (first/last mile).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::shared::config::{PedestrianConfig, ValhallaConfig, WheelchairConfig};

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

/// Who is walking: the costing profile and speed of one request.
#[derive(Clone, Copy)]
pub struct Walker<'a> {
    /// Default pedestrian profile (`pedestrian` in `config.yaml`).
    pub pedestrian: &'a PedestrianConfig,
    /// Wheelchair profile, replacing the pedestrian one — speed included —
    /// when the request asks for accessible routing.
    pub wheelchair: Option<&'a WheelchairConfig>,
    /// Walking speed the request asked for (km/h), if any.
    pub requested_speed: Option<f64>,
}

impl Walker<'_> {
    /// Walking speed in km/h: the wheelchair profile's, else the requested
    /// one, else the configured default.
    pub fn speed_kmh(&self) -> f64 {
        match self.wheelchair {
            Some(wc) => wc.walking_speed,
            None => self
                .requested_speed
                .unwrap_or(self.pedestrian.walking_speed),
        }
    }
}

/// Pedestrian costing options shared by every walk Glove asks Valhalla for,
/// so a walk is timed the same way whether RAPTOR is choosing a stop, the
/// journey is being drawn, or the walk endpoint answers.
///
/// `indoor_friendly` is for station transfers: stairs, escalators and
/// elevators are the normal path through underground passages, so they cost
/// nothing extra.
pub fn pedestrian_costing(walker: &Walker<'_>, indoor_friendly: bool) -> serde_json::Value {
    let mut opts = if let Some(wc) = walker.wheelchair {
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
        serde_json::json!({
            "pedestrian": {
                "step_penalty": 0,
                "elevator_penalty": 0,
                "use_tunnels": 1.0
            }
        })
    } else {
        serde_json::json!({
            "pedestrian": {
                "step_penalty": walker.pedestrian.step_penalty,
                "elevator_penalty": walker.pedestrian.elevator_penalty
            }
        })
    };
    opts["pedestrian"]["walking_speed"] = serde_json::json!(walker.speed_kmh().clamp(0.5, 25.5));
    opts
}

#[derive(Serialize)]
struct MatrixRequest {
    sources: Vec<Location>,
    targets: Vec<Location>,
    costing: String,
    costing_options: serde_json::Value,
}

#[derive(Deserialize)]
struct MatrixResponse {
    sources_to_targets: Vec<Vec<MatrixCell>>,
}

#[derive(Deserialize)]
struct MatrixCell {
    /// Seconds; `null` when Valhalla finds no path between the two points.
    time: Option<f64>,
}

/// Walking-time matrix between two sets of `(lon, lat)` points, for first and
/// last-mile walks.
///
/// Row `i`, column `j` is the walk from `sources[i]` to `targets[j]` in
/// seconds, `None` where Valhalla finds no path. The whole call is `None` when
/// Valhalla is unreachable or rejects the request (too many locations), so the
/// caller can keep its own estimate.
pub async fn pedestrian_durations(
    valhalla: &ValhallaConfig,
    sources: &[(f64, f64)],
    targets: &[(f64, f64)],
    walker: &Walker<'_>,
) -> Option<Vec<Vec<Option<u32>>>> {
    let locations = |points: &[(f64, f64)]| -> Vec<Location> {
        points
            .iter()
            .map(|&(lon, lat)| Location { lat, lon })
            .collect()
    };
    let req = MatrixRequest {
        sources: locations(sources),
        targets: locations(targets),
        costing: "pedestrian".to_string(),
        costing_options: pedestrian_costing(walker, false),
    };

    let url = format!("{}/sources_to_targets", valhalla.base_url());
    let resp = crate::shared::http::client()
        .post(&url)
        .timeout(valhalla.pedestrian_timeout())
        .json(&req)
        .send()
        .await
        .inspect_err(|e| tracing::debug!("Valhalla matrix unreachable: {e}"))
        .ok()?;
    if !resp.status().is_success() {
        tracing::debug!("Valhalla matrix returned {}", resp.status());
        return None;
    }
    let matrix: MatrixResponse = resp
        .json()
        .await
        .inspect_err(|e| tracing::debug!("Invalid Valhalla matrix response: {e}"))
        .ok()?;
    Some(
        matrix
            .sources_to_targets
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| cell.time.map(|t| t.ceil() as u32))
                    .collect()
            })
            .collect(),
    )
}

/// Compute a pedestrian route between two coordinates via Valhalla.
///
/// When `indoor_friendly` is true, step and elevator penalties are removed
/// to favor underground passages in stations. This is used for transfer
/// sections where stairs/escalators/elevators are the normal path.
///
/// Returns `None` if Valhalla is unreachable or returns an error.
pub async fn pedestrian_route(
    valhalla: &ValhallaConfig,
    from: (f64, f64), // (lon, lat)
    to: (f64, f64),   // (lon, lat)
    walker: &Walker<'_>,
    indoor_friendly: bool,
    language: Option<&str>,
) -> Option<WalkLeg> {
    let costing_options = Some(pedestrian_costing(walker, indoor_friendly));

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

    let url = format!("{}/route", valhalla.base_url());
    let resp = crate::shared::http::client()
        .post(&url)
        .timeout(valhalla.pedestrian_timeout())
        .json(&req)
        .send()
        .await
        .inspect_err(|e| tracing::debug!("Valhalla pedestrian route unreachable: {e}"))
        .ok()?;
    if !resp.status().is_success() {
        tracing::debug!("Valhalla pedestrian route returned {}", resp.status());
        return None;
    }

    let route: RouteResponse = resp
        .json()
        .await
        .inspect_err(|e| tracing::debug!("Invalid Valhalla pedestrian response: {e}"))
        .ok()?;
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

    /// Every pair is a 300 s walk, except identical points (0 s, as Valhalla
    /// answers when an address sits on the stop) and any point at longitude 0,
    /// which Valhalla "cannot route" (`null`).
    #[post("/sources_to_targets")]
    async fn matrix_handler(body: web::Json<serde_json::Value>) -> HttpResponse {
        let points = |key: &str| body[key].as_array().cloned().unwrap_or_default();
        let unroutable = |p: &serde_json::Value| p["lon"].as_f64() == Some(0.0);
        let rows: Vec<Vec<serde_json::Value>> = points("sources")
            .iter()
            .map(|source| {
                points("targets")
                    .iter()
                    .map(|target| {
                        let time = if unroutable(source) || unroutable(target) {
                            serde_json::Value::Null
                        } else if source == target {
                            serde_json::json!(0.0)
                        } else {
                            serde_json::json!(300.0)
                        };
                        serde_json::json!({ "time": time, "distance": 0.4 })
                    })
                    .collect()
            })
            .collect();
        HttpResponse::Ok().json(serde_json::json!({ "sources_to_targets": rows }))
    }

    /// A Valhalla configuration pointing at `base` (`http://host:port`).
    pub fn valhalla_at(base: &str) -> crate::shared::config::ValhallaConfig {
        let (host, port) = base
            .trim_start_matches("http://")
            .split_once(':')
            .expect("base is http://host:port");
        crate::shared::config::ValhallaConfig {
            host: host.to_string(),
            port: port.parse().expect("numeric port"),
            ..Default::default()
        }
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
                let server = HttpServer::new(|| {
                    App::new()
                        .service(route_handler)
                        .service(height_handler)
                        .service(matrix_handler)
                })
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
    use crate::shared::config::AppConfig;

    fn walker<'a>(
        cfg: &'a AppConfig,
        requested_speed: Option<f64>,
        wheelchair: bool,
    ) -> Walker<'a> {
        Walker {
            pedestrian: &cfg.pedestrian,
            wheelchair: wheelchair.then_some(&cfg.wheelchair),
            requested_speed,
        }
    }

    #[test]
    fn outdoor_costing_uses_the_configured_pedestrian_profile() {
        let mut cfg = AppConfig::default();
        cfg.pedestrian.step_penalty = 45.0;
        cfg.pedestrian.elevator_penalty = 90.0;
        cfg.pedestrian.walking_speed = 4.2;
        let opts = pedestrian_costing(&walker(&cfg, None, false), false);
        assert_eq!(opts["pedestrian"]["step_penalty"], 45.0);
        assert_eq!(opts["pedestrian"]["elevator_penalty"], 90.0);
        assert_eq!(opts["pedestrian"]["walking_speed"], 4.2);
    }

    #[test]
    fn a_requested_speed_beats_the_default_and_is_clamped() {
        let cfg = AppConfig::default();
        let opts = pedestrian_costing(&walker(&cfg, Some(6.0), false), false);
        assert_eq!(opts["pedestrian"]["walking_speed"], 6.0);
        let opts = pedestrian_costing(&walker(&cfg, Some(99.0), false), false);
        assert_eq!(opts["pedestrian"]["walking_speed"], 25.5);
    }

    #[test]
    fn indoor_costing_ignores_stairs_and_elevators() {
        let cfg = AppConfig::default();
        let opts = pedestrian_costing(&walker(&cfg, None, false), true);
        assert_eq!(opts["pedestrian"]["step_penalty"], 0);
        assert_eq!(opts["pedestrian"]["elevator_penalty"], 0);
    }

    #[test]
    fn the_wheelchair_profile_replaces_penalties_and_speed() {
        let cfg = AppConfig::default();
        let opts = pedestrian_costing(&walker(&cfg, Some(99.0), true), false);
        assert_eq!(
            opts["pedestrian"]["step_penalty"],
            cfg.wheelchair.step_penalty
        );
        assert_eq!(
            opts["pedestrian"]["walking_speed"],
            cfg.wheelchair.walking_speed
        );
    }

    #[actix_web::test]
    async fn pedestrian_route_is_none_when_valhalla_is_down() {
        let cfg = AppConfig::default();
        let valhalla = test_support::valhalla_at("http://127.0.0.1:1");
        let out = pedestrian_route(
            &valhalla,
            (2.3, 48.8),
            (2.4, 48.9),
            &walker(&cfg, None, false),
            false,
            None,
        )
        .await;
        assert!(out.is_none());
    }

    #[actix_web::test]
    async fn pedestrian_route_success_against_mock_valhalla() {
        let cfg = AppConfig::default();
        let valhalla = test_support::valhalla_at(&test_support::spawn_mock_valhalla());
        let leg = pedestrian_route(
            &valhalla,
            (2.3, 48.8),
            (2.4, 48.9),
            &walker(&cfg, Some(5.0), false),
            false,
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
