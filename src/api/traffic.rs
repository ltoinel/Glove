//! Real-time road traffic overlay: geometry + states endpoints and the refresh
//! loop feeding them.
//!
//! The payload is split along its natural lifetime, because sending both parts
//! together meant re-sending ~1 MB of unchanged polylines every minute:
//! - `GET /api/traffic/geometry` — the road network, built once at startup and
//!   served with a long `Cache-Control` (it only changes when the operator
//!   publishes a new MIF/MID pair, i.e. across restarts).
//! - `GET /api/traffic/states` — the live states and events, a few tens of kB,
//!   refreshed every `traffic.refresh_secs`.
//!
//! Both bodies are serialized once (at startup / at each refresh) and handed
//! out verbatim: a snapshot covers several thousand segments, so serializing
//! per request would dominate the handler's cost.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwapOption;
use serde::Serialize;
use tracing::{debug, info, warn};
use utoipa::ToSchema;

use crate::config::TrafficConfig;
use crate::traffic::{TrafficEvent, TrafficGeometry, build_snapshot};

/// Timeout for a single dynamic-feed fetch.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Path of the dynamic segment states, relative to `traffic.base_url`.
const STATES_PATH: &str = "xml/segments_dyn.xml";

/// Path of the dynamic events feed, relative to `traffic.base_url`.
const EVENTS_PATH: &str = "xml/evenements.xml";

/// Browser cache lifetime of the geometry. The network is stable across a
/// server's lifetime, and a restart with new data changes nothing for a client
/// that keeps a stale copy for a day — segments only ever gain or lose states.
const GEOMETRY_CACHE_SECS: u32 = 86_400;

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

/// Response of `GET /api/traffic/geometry`.
#[derive(Serialize, ToSchema)]
pub struct TrafficGeometryResponse {
    /// `false` when the overlay is disabled in config; `segments` is then empty.
    pub enabled: bool,
    /// `ID_SEGMENT` → polyline as `[[lat, lon], ...]`, in WGS84.
    pub segments: HashMap<u32, Vec<[f64; 2]>>,
}

/// Response of `GET /api/traffic/states`.
#[derive(Serialize, ToSchema)]
pub struct TrafficStatesResponse {
    /// `false` when the overlay is disabled in config.
    pub enabled: bool,
    /// RFC3339 timestamp of the snapshot, absent while disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// `ID_SEGMENT` → `"fluid"` | `"jam"` | `"closed"`, restricted to segments
    /// present in the geometry response.
    pub states: HashMap<u32, &'static str>,
    /// Located traffic events (roadworks, accidents…).
    pub events: Vec<TrafficEvent>,
}

/// Body served by both endpoints when the overlay is disabled.
fn disabled_body(kind: &str) -> HttpResponse {
    match kind {
        "geometry" => HttpResponse::Ok().json(TrafficGeometryResponse {
            enabled: false,
            segments: HashMap::new(),
        }),
        _ => HttpResponse::Ok().json(TrafficStatesResponse {
            enabled: false,
            updated_at: None,
            states: HashMap::new(),
            events: Vec::new(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared traffic state: the immutable geometry body plus the latest states
/// body, both already serialized.
///
/// The states body is swapped atomically by the refresh loop, so readers never
/// block and always see a complete snapshot (same lock-free approach as the
/// RAPTOR index hot-reload).
pub struct TrafficService {
    enabled: bool,
    /// Serialized [`TrafficGeometryResponse`], built once at startup.
    geometry: Option<web::Bytes>,
    /// Serialized [`TrafficStatesResponse`]; `None` until the first fetch.
    states: ArcSwapOption<web::Bytes>,
}

impl TrafficService {
    /// A service that reports the overlay as disabled and never polls.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            geometry: None,
            states: ArcSwapOption::empty(),
        }
    }

    /// An enabled service holding its geometry, still waiting for live states.
    fn enabled(geometry: web::Bytes) -> Self {
        Self {
            enabled: true,
            geometry: Some(geometry),
            states: ArcSwapOption::empty(),
        }
    }

    /// Publish a freshly serialized states snapshot.
    fn publish_states(&self, body: web::Bytes) {
        self.states.store(Some(Arc::new(body)));
    }

    /// The current states body, if one has been published.
    fn states_body(&self) -> Option<web::Bytes> {
        self.states.load_full().map(|b| b.as_ref().clone())
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Return the static road-network geometry, cacheable for a day.
#[utoipa::path(
    get,
    path = "/api/traffic/geometry",
    responses(
        (status = 200, description = "Road network polylines", body = TrafficGeometryResponse),
    ),
    tag = "Traffic"
)]
#[get("/api/traffic/geometry")]
pub async fn get_traffic_geometry(service: web::Data<TrafficService>) -> HttpResponse {
    match &service.geometry {
        Some(body) => HttpResponse::Ok()
            .content_type("application/json")
            .append_header((
                "Cache-Control",
                format!("public, max-age={GEOMETRY_CACHE_SECS}"),
            ))
            .body(body.clone()),
        None => disabled_body("geometry"),
    }
}

/// Return the latest segment states and events.
#[utoipa::path(
    get,
    path = "/api/traffic/states",
    responses(
        (status = 200, description = "Latest traffic states and events", body = TrafficStatesResponse),
        (status = 503, description = "Overlay enabled but no snapshot fetched yet"),
    ),
    tag = "Traffic"
)]
#[get("/api/traffic/states")]
pub async fn get_traffic_states(service: web::Data<TrafficService>) -> HttpResponse {
    if !service.enabled {
        return disabled_body("states");
    }
    match service.states_body() {
        Some(body) => HttpResponse::Ok()
            .content_type("application/json")
            .append_header(("Cache-Control", "no-cache"))
            .body(body),
        None => HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "error": {
                "id": "traffic_unavailable",
                "message": "Traffic snapshot not available yet"
            }
        })),
    }
}

// ---------------------------------------------------------------------------
// Startup and refresh loop
// ---------------------------------------------------------------------------

/// Load the static geometry, serialize it once and start the refresh loop.
///
/// Returns a service that reports the overlay as disabled when
/// `traffic.enabled` is `false` or the geometry cannot be loaded — a missing
/// `Segment.mif`/`.mid` pair degrades the overlay, never the server.
pub fn start(config: &TrafficConfig, sytadin_dir: &std::path::Path) -> web::Data<TrafficService> {
    if !config.enabled {
        return web::Data::new(TrafficService::disabled());
    }
    let geometry = match TrafficGeometry::load(sytadin_dir) {
        Ok(geometry) if !geometry.is_empty() => Arc::new(geometry),
        Ok(_) => {
            warn!(
                "Traffic overlay disabled: no segment geometry in {}",
                sytadin_dir.display()
            );
            return web::Data::new(TrafficService::disabled());
        }
        Err(e) => {
            warn!("Traffic overlay disabled: {e} (run `bin/download.sh traffic`)");
            return web::Data::new(TrafficService::disabled());
        }
    };

    let body = match serialize_geometry(&geometry) {
        Ok(body) => body,
        Err(e) => {
            warn!("Traffic overlay disabled: {e}");
            return web::Data::new(TrafficService::disabled());
        }
    };
    info!(
        "{} traffic segments indexed ({} kB served once)",
        geometry.len(),
        body.len() / 1024
    );

    let service = web::Data::new(TrafficService::enabled(body));
    spawn_refresh_loop(service.clone(), geometry, config);
    service
}

/// Serialize the geometry response. The clone is paid once at startup; only the
/// resulting bytes are kept afterwards.
fn serialize_geometry(geometry: &TrafficGeometry) -> Result<web::Bytes, String> {
    let response = TrafficGeometryResponse {
        enabled: true,
        segments: geometry.polylines().clone(),
    };
    serde_json::to_vec(&response)
        .map(web::Bytes::from)
        .map_err(|e| format!("serialize geometry: {e}"))
}

/// Poll the diffusion feed every `refresh_secs` for the lifetime of the server.
fn spawn_refresh_loop(
    service: web::Data<TrafficService>,
    geometry: Arc<TrafficGeometry>,
    config: &TrafficConfig,
) {
    let base_url = config.base_url.trim_end_matches('/').to_string();
    let interval = Duration::from_secs(config.refresh_secs.max(1));

    actix_web::rt::spawn(async move {
        let client = match reqwest::Client::builder().timeout(FETCH_TIMEOUT).build() {
            Ok(client) => client,
            Err(e) => {
                warn!("Traffic refresh disabled: cannot build HTTP client: {e}");
                return;
            }
        };
        loop {
            match refresh_once(&client, &base_url, &geometry).await {
                Ok(body) => {
                    debug!("Traffic states refreshed ({} bytes)", body.len());
                    service.publish_states(body);
                }
                Err(e) => warn!("Traffic refresh failed: {e}"),
            }
            actix_web::rt::time::sleep(interval).await;
        }
    });
}

/// Fetch both dynamic feeds and serialize the resulting states snapshot.
async fn refresh_once(
    client: &reqwest::Client,
    base_url: &str,
    geometry: &TrafficGeometry,
) -> Result<web::Bytes, String> {
    let states_xml = fetch_feed(client, base_url, STATES_PATH).await?;
    let events_xml = fetch_feed(client, base_url, EVENTS_PATH).await?;
    let snapshot = build_snapshot(geometry, &states_xml, &events_xml);
    let response = TrafficStatesResponse {
        enabled: true,
        // Prefer the feed's own publication time; fall back to our clock only
        // when the feed omits it.
        updated_at: snapshot
            .diffused_at
            .or_else(|| Some(chrono::Utc::now().to_rfc3339())),
        states: snapshot.states,
        events: snapshot.events,
    };
    serde_json::to_vec(&response)
        .map(web::Bytes::from)
        .map_err(|e| format!("serialize states: {e}"))
}

/// Download one XML feed from `{base_url}/{path}`.
async fn fetch_feed(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
) -> Result<Vec<u8>, String> {
    let url = format!("{base_url}/{path}");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("GET {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GET {url}: upstream returned {}", resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("read {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpServer, test};

    /// Write a minimal MIF/MID pair holding a single Lambert II étendu segment.
    fn write_minimal_geometry(dir: &std::path::Path) {
        std::fs::write(
            dir.join("Segment.mif"),
            "version 300\nColumns 1\n  ID_SEGMENT Integer\nData\n\n\
             pline 2\n607478.71 2419967.39\n607515.31 2419756.07\n",
        )
        .unwrap();
        std::fs::write(dir.join("Segment.mid"), "20,\"SEG/N6\"\n").unwrap();
    }

    #[actix_web::test]
    async fn disabled_service_reports_disabled_on_both_endpoints() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(TrafficService::disabled()))
                .service(get_traffic_geometry)
                .service(get_traffic_states),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/traffic/geometry")
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["enabled"], false);
        assert!(body["segments"].as_object().unwrap().is_empty());

        let req = test::TestRequest::get()
            .uri("/api/traffic/states")
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["enabled"], false);
        assert!(body["states"].as_object().unwrap().is_empty());
    }

    #[actix_web::test]
    async fn geometry_is_served_with_a_long_cache_header() {
        let dir = tempfile::tempdir().unwrap();
        write_minimal_geometry(dir.path());
        let geometry = TrafficGeometry::load(dir.path()).unwrap();
        let service = web::Data::new(TrafficService::enabled(
            serialize_geometry(&geometry).unwrap(),
        ));

        let app =
            test::init_service(App::new().app_data(service).service(get_traffic_geometry)).await;
        let req = test::TestRequest::get()
            .uri("/api/traffic/geometry")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let cache = resp.headers().get("Cache-Control").unwrap();
        assert!(cache.to_str().unwrap().contains("max-age=86400"));

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["enabled"], true);
        assert_eq!(body["segments"]["20"].as_array().unwrap().len(), 2);
    }

    #[actix_web::test]
    async fn states_returns_503_before_first_snapshot() {
        let service = web::Data::new(TrafficService::enabled(web::Bytes::from_static(b"{}")));
        let app =
            test::init_service(App::new().app_data(service).service(get_traffic_states)).await;
        let req = test::TestRequest::get()
            .uri("/api/traffic/states")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
    }

    #[actix_web::test]
    async fn published_states_are_served_as_is() {
        let service = web::Data::new(TrafficService::enabled(web::Bytes::from_static(b"{}")));
        service.publish_states(web::Bytes::from_static(br#"{"enabled":true}"#));
        let app =
            test::init_service(App::new().app_data(service).service(get_traffic_states)).await;
        let req = test::TestRequest::get()
            .uri("/api/traffic/states")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(&body[..], br#"{"enabled":true}"#);
    }

    #[actix_web::test]
    async fn start_returns_disabled_service_when_config_is_off() {
        let config = TrafficConfig {
            enabled: false,
            ..TrafficConfig::default()
        };
        let service = start(&config, std::path::Path::new("/nonexistent"));
        assert!(!service.enabled);
    }

    #[actix_web::test]
    async fn start_degrades_to_disabled_when_geometry_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let config = TrafficConfig {
            enabled: true,
            ..TrafficConfig::default()
        };
        let service = start(&config, dir.path());
        assert!(!service.enabled);
    }

    /// Serve the two dynamic feeds from a local server and return its base URL.
    fn spawn_mock_feed_server(states: &'static str, events: &'static str) -> String {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let sys = actix_web::rt::System::new();
            sys.block_on(async move {
                let server = HttpServer::new(move || {
                    App::new()
                        .route(
                            "/xml/segments_dyn.xml",
                            web::get().to(move || async move { HttpResponse::Ok().body(states) }),
                        )
                        .route(
                            "/xml/evenements.xml",
                            web::get().to(move || async move { HttpResponse::Ok().body(events) }),
                        )
                })
                .listen(listener)
                .unwrap()
                .workers(1)
                .run();
                let _ = server.await;
            });
        });
        std::thread::sleep(Duration::from_millis(150));
        format!("http://127.0.0.1:{port}")
    }

    #[actix_web::test]
    async fn refresh_once_serializes_states_without_coordinates() {
        let dir = tempfile::tempdir().unwrap();
        write_minimal_geometry(dir.path());
        let geometry = TrafficGeometry::load(dir.path()).unwrap();

        let base_url = spawn_mock_feed_server(
            r#"<R><SegmentDynamique ID_SEGMENT="20">
                <EtatTrafic>Bouchon</EtatTrafic></SegmentDynamique>
                <SegmentDynamique ID_SEGMENT="99">
                <EtatTrafic>Fluide</EtatTrafic></SegmentDynamique></R>"#,
            r#"<R></R>"#,
        );
        let client = reqwest::Client::new();
        let body = refresh_once(&client, &base_url, &geometry).await.unwrap();

        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["enabled"], true);
        assert!(json["updated_at"].is_string());
        // Segment 99 has no geometry, so no client could draw it.
        assert_eq!(json["states"].as_object().unwrap().len(), 1);
        assert_eq!(json["states"]["20"], "jam");
        assert!(!body.windows(6).any(|w| w == b"coords"));
    }

    #[actix_web::test]
    async fn refresh_once_reports_unreachable_upstream() {
        let dir = tempfile::tempdir().unwrap();
        write_minimal_geometry(dir.path());
        let geometry = TrafficGeometry::load(dir.path()).unwrap();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let err = refresh_once(&client, "http://127.0.0.1:1", &geometry)
            .await
            .unwrap_err();
        assert!(err.contains("segments_dyn.xml"), "unexpected error: {err}");
    }
}
