//! Pedestrian journey endpoint via Valhalla routing engine.
//!
//! Calls Valhalla's `/route` API with `"pedestrian"` costing to compute
//! walking directions between two geographic coordinates.

use actix_web::{HttpResponse, get, web};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::valhalla::{DirectionsOptions, Location, RouteRequest, RouteResponse};
use crate::config::AppConfig;
use crate::util::parse_from_to;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/journeys/walk`.
///
/// Coordinates are passed as `lon;lat` strings.
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
    /// Language for maneuver instructions (e.g. "fr-FR", "en-US").
    pub language: Option<String>,
    /// Enable wheelchair-accessible routing (avoid stairs, limit grade).
    pub wheelchair: Option<bool>,
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
    /// Index into the encoded shape where this maneuver begins.
    pub begin_shape_index: usize,
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
    let valhalla_req = build_walk_request(&query, &config, from_lat, from_lon, to_lat, to_lon);

    let valhalla_resp = match call_valhalla(&valhalla_url, &valhalla_req).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let trip = &valhalla_resp.trip;
    let Some(leg) = trip.legs.first() else {
        return valhalla_error(502, "Valhalla returned no route legs".into());
    };

    let journey = WalkJourney {
        duration: trip.summary.time as u32,
        distance: (trip.summary.length * 1000.0) as u32,
        shape: leg.shape.clone(),
        maneuvers: if query.maneuvers.unwrap_or(false) {
            Some(convert_maneuvers(&leg.maneuvers))
        } else {
            None
        },
    };

    HttpResponse::Ok().json(WalkResponse {
        journeys: vec![journey],
    })
}

/// Build the Valhalla `/route` request body for a pedestrian query, applying
/// wheelchair-specific costing options when enabled.
fn build_walk_request(
    query: &WalkQuery,
    config: &AppConfig,
    from_lat: f64,
    from_lon: f64,
    to_lat: f64,
    to_lon: f64,
) -> RouteRequest {
    let wheelchair = query.wheelchair.unwrap_or(false);
    let mut opts = if wheelchair {
        let wc = &config.wheelchair;
        serde_json::json!({
            "pedestrian": {
                "step_penalty": wc.step_penalty,
                "max_grade": wc.max_grade,
                "use_hills": wc.use_hills,
                "elevator_penalty": wc.elevator_penalty
            }
        })
    } else {
        serde_json::json!({
            "pedestrian": {
                "step_penalty": 30,
                "elevator_penalty": 60
            }
        })
    };
    let effective_speed = if wheelchair {
        Some(config.wheelchair.walking_speed)
    } else {
        query.walking_speed
    };
    if let Some(speed) = effective_speed {
        opts["pedestrian"]["walking_speed"] = serde_json::json!(speed.clamp(0.5, 25.5));
    }

    RouteRequest {
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
        costing: "pedestrian".to_string(),
        costing_options: Some(opts),
        directions_options: DirectionsOptions {
            units: "kilometers".to_string(),
            language: query.language.clone(),
        },
    }
}

/// Send a routing request to Valhalla and decode its response, converting
/// transport-level failures into HTTP 502 error responses.
async fn call_valhalla(url: &str, req: &RouteRequest) -> Result<RouteResponse, HttpResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(req)
        .send()
        .await
        .map_err(|e| valhalla_error(502, format!("Failed to reach Valhalla: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(valhalla_error(
            502,
            format!("Valhalla returned {status}: {body}"),
        ));
    }

    resp.json::<RouteResponse>()
        .await
        .map_err(|e| valhalla_error(502, format!("Invalid Valhalla response: {e}")))
}

fn valhalla_error(_status: u16, message: String) -> HttpResponse {
    HttpResponse::BadGateway().json(serde_json::json!({
        "error": { "id": "valhalla_error", "message": message }
    }))
}

/// Convert Valhalla maneuvers into the API's compact representation.
fn convert_maneuvers(src: &[super::valhalla::RawManeuver]) -> Vec<Maneuver> {
    src.iter()
        .map(|m| Maneuver {
            instruction: m.instruction.clone(),
            maneuver_type: m.maneuver_type,
            distance: (m.length * 1000.0) as u32,
            duration: m.time as u32,
            begin_shape_index: m.begin_shape_index,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use std::path::Path;

    fn make_config(wc_speed: f64, step_penalty: f64) -> AppConfig {
        // Load defaults from a non-existent path, then tweak wheelchair fields.
        let mut cfg = AppConfig::load(Path::new("/nonexistent-test-path-glove.yaml"));
        cfg.wheelchair.step_penalty = step_penalty;
        cfg.wheelchair.walking_speed = wc_speed;
        cfg
    }

    #[test]
    fn convert_maneuvers_scales_distance_and_time() {
        let raw = vec![super::super::valhalla::RawManeuver {
            instruction: "Turn left".into(),
            length: 0.250, // km
            time: 30.0,
            maneuver_type: 5,
            begin_shape_index: 7,
        }];
        let out = convert_maneuvers(&raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].distance, 250);
        assert_eq!(out[0].duration, 30);
        assert_eq!(out[0].maneuver_type, 5);
        assert_eq!(out[0].begin_shape_index, 7);
    }

    #[test]
    fn convert_maneuvers_handles_empty() {
        assert!(convert_maneuvers(&[]).is_empty());
    }

    #[test]
    fn valhalla_error_returns_502() {
        let resp = valhalla_error(0, "boom".into());
        assert_eq!(resp.status(), 502);
    }

    #[test]
    fn build_walk_request_default_costing() {
        let cfg = make_config(3.5, 99.0);
        let query = WalkQuery {
            from: "2.3;48.8".into(),
            to: "2.4;48.9".into(),
            walking_speed: Some(4.5),
            maneuvers: None,
            language: Some("fr-FR".into()),
            wheelchair: Some(false),
        };
        let req = build_walk_request(&query, &cfg, 48.8, 2.3, 48.9, 2.4);
        assert_eq!(req.costing, "pedestrian");
        let opts = req.costing_options.as_ref().unwrap();
        assert_eq!(opts["pedestrian"]["step_penalty"], 30);
        assert_eq!(opts["pedestrian"]["elevator_penalty"], 60);
        assert_eq!(opts["pedestrian"]["walking_speed"], 4.5);
        assert_eq!(req.directions_options.units, "kilometers");
        assert_eq!(req.directions_options.language.as_deref(), Some("fr-FR"));
    }

    #[test]
    fn build_walk_request_wheelchair_uses_wc_speed_and_penalties() {
        let cfg = make_config(3.0, 99.0);
        let query = WalkQuery {
            from: "2.3;48.8".into(),
            to: "2.4;48.9".into(),
            walking_speed: Some(8.0), // ignored when wheelchair is on
            maneuvers: None,
            language: None,
            wheelchair: Some(true),
        };
        let req = build_walk_request(&query, &cfg, 48.8, 2.3, 48.9, 2.4);
        let opts = req.costing_options.as_ref().unwrap();
        assert_eq!(opts["pedestrian"]["step_penalty"], 99.0);
        assert_eq!(opts["pedestrian"]["walking_speed"], 3.0);
    }

    fn unreachable_config() -> AppConfig {
        let mut cfg = AppConfig::load(Path::new("/no-such-file.yaml"));
        cfg.valhalla.host = "127.0.0.1".into();
        cfg.valhalla.port = 1;
        cfg
    }

    #[actix_web::test]
    async fn get_walk_rejects_bad_from() {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(unreachable_config()))
                .service(get_walk),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/walk?from=bad&to=2.4;48.9")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_walk_unreachable_valhalla_returns_502() {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(unreachable_config()))
                .service(get_walk),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/walk?from=2.3;48.8&to=2.4;48.9")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 502);
    }

    #[actix_web::test]
    async fn get_walk_success_against_mock() {
        let base = super::super::valhalla::test_support::spawn_mock_valhalla();
        // Extract host:port from "http://127.0.0.1:PORT"
        let rest = base.strip_prefix("http://").unwrap();
        let (host, port_str) = rest.split_once(':').unwrap();
        let mut cfg = unreachable_config();
        cfg.valhalla.host = host.into();
        cfg.valhalla.port = port_str.parse().unwrap();

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(cfg))
                .service(get_walk),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/walk?from=2.3;48.8&to=2.4;48.9&maneuvers=true")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert!(body["journeys"][0]["duration"].as_u64().unwrap() > 0);
        assert!(body["journeys"][0]["maneuvers"].is_array());
    }

    #[actix_web::test]
    async fn get_walk_with_maneuvers_and_wheelchair() {
        // Exercises the maneuvers=true + wheelchair=true query path
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(unreachable_config()))
                .service(get_walk),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/walk?from=2.3;48.8&to=2.4;48.9&maneuvers=true&wheelchair=true")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 502);
    }

    #[test]
    fn build_walk_request_clamps_speed() {
        let cfg = make_config(3.0, 30.0);
        let query = WalkQuery {
            from: "2.3;48.8".into(),
            to: "2.4;48.9".into(),
            walking_speed: Some(99.0), // outside Valhalla range
            maneuvers: None,
            language: None,
            wheelchair: None,
        };
        let req = build_walk_request(&query, &cfg, 48.8, 2.3, 48.9, 2.4);
        let opts = req.costing_options.as_ref().unwrap();
        assert_eq!(opts["pedestrian"]["walking_speed"], 25.5);
    }
}
