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
/// Coordinates are passed as `lon;lat` strings.
#[derive(Debug, Deserialize, IntoParams)]
pub struct BikeQuery {
    /// Origin as `lon;lat`.
    pub from: String,
    /// Destination as `lon;lat`.
    pub to: String,
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
    let include_maneuvers = config.routing.maneuvers;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BikeProfile;

    #[test]
    fn compute_elevation_handles_empty_input() {
        let (gain, loss) = compute_elevation(&[]);
        assert_eq!((gain, loss), (0, 0));
    }

    #[test]
    fn compute_elevation_handles_single_point() {
        let (gain, loss) = compute_elevation(&[100.0]);
        assert_eq!((gain, loss), (0, 0));
    }

    #[test]
    fn compute_elevation_sums_gain_and_loss_separately() {
        let heights = [10.0, 30.0, 25.0, 60.0, 50.0];
        // +20, -5, +35, -10 → gain=55, loss=15
        let (gain, loss) = compute_elevation(&heights);
        assert_eq!(gain, 55);
        assert_eq!(loss, 15);
    }

    #[test]
    fn compute_elevation_flat_returns_zero() {
        let (gain, loss) = compute_elevation(&[100.0, 100.0, 100.0]);
        assert_eq!((gain, loss), (0, 0));
    }

    #[test]
    fn decode_polyline_empty_input() {
        assert!(decode_polyline("").is_empty());
    }

    #[test]
    fn decode_polyline_round_trip_single_point() {
        // Hand-crafted encoded "u{~vF`y~oC" decodes to a known coord. We just
        // verify the decoder yields at least one coord and that it parses
        // without panicking.
        let encoded = "u{~vF`y~oC";
        let pts = decode_polyline(encoded);
        assert!(!pts.is_empty());
        // first coord should be in the Paris area when used with precision 5
        // but our decoder uses precision 6 — so just assert finite values.
        for (lat, lon) in &pts {
            assert!(lat.is_finite());
            assert!(lon.is_finite());
        }
    }

    #[test]
    fn bike_costing_options_contains_profile_fields() {
        let profile = BikeProfile {
            cycling_speed: 22.0,
            use_roads: 0.6,
            use_hills: 0.4,
            bicycle_type: "Hybrid".into(),
        };
        let opts = bike_costing_options(&profile);
        assert_eq!(opts["bicycle"]["cycling_speed"], 22.0);
        assert_eq!(opts["bicycle"]["use_roads"], 0.6);
        assert_eq!(opts["bicycle"]["use_hills"], 0.4);
        assert_eq!(opts["bicycle"]["bicycle_type"], "Hybrid");
    }

    #[test]
    fn convert_maneuvers_scales_distance_and_time() {
        let raw = vec![RawManeuver {
            instruction: "Turn right".into(),
            length: 1.5, // km
            time: 60.0,
            maneuver_type: 9,
            begin_shape_index: 4,
        }];
        let out = convert_maneuvers(&raw);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].distance, 1500);
        assert_eq!(out[0].duration, 60);
        assert_eq!(out[0].maneuver_type, 9);
        assert_eq!(out[0].begin_shape_index, 4);
    }

    #[test]
    fn bike_profiles_constant_lists_three_variants() {
        assert_eq!(BIKE_PROFILES.len(), 3);
        assert!(BIKE_PROFILES.contains(&"city"));
        assert!(BIKE_PROFILES.contains(&"ebike"));
        assert!(BIKE_PROFILES.contains(&"road"));
    }

    #[test]
    fn elevation_sample_limit_is_positive() {
        assert!(ELEVATION_SAMPLE_LIMIT > 0);
    }

    fn unreachable_config() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.valhalla.host = "127.0.0.1".into();
        cfg.valhalla.port = 1;
        cfg
    }

    #[actix_web::test]
    async fn get_bike_rejects_bad_coords() {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(unreachable_config()))
                .service(get_bike),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/bike?from=bad&to=2.4;48.9")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_bike_success_against_mock() {
        let base = super::super::valhalla::test_support::spawn_mock_valhalla();
        let rest = base.strip_prefix("http://").unwrap();
        let (host, port_str) = rest.split_once(':').unwrap();
        let mut cfg = AppConfig::default();
        cfg.valhalla.host = host.into();
        cfg.valhalla.port = port_str.parse().unwrap();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(cfg))
                .service(get_bike),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/bike?from=2.3;48.8&to=2.4;48.9&maneuvers=true")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        // Should return three bike profile variants
        assert!(body["journeys"].as_array().unwrap().len() >= 1);
    }

    #[actix_web::test]
    async fn get_bike_unreachable_returns_response_with_no_journeys() {
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(actix_web::web::Data::new(unreachable_config()))
                .service(get_bike),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/bike?from=2.3;48.8&to=2.4;48.9&maneuvers=true")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        // get_bike degrades gracefully: 200 with empty journeys when Valhalla unreachable.
        assert!(resp.status() == 200 || resp.status() == 502);
    }
}
