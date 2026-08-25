//! Back-office CRUD for operator-authored disruptions.
//!
//! Reads are open, writes are guarded by the same `X-Api-Key` header as
//! `POST /api/gtfs/reload`: both change what the engine answers, and splitting
//! them across two credentials would give operators one more secret to lose.
//!
//! Nothing here touches the RAPTOR index. A write updates the catalog, and the
//! next journey query resolves it — see
//! [`crate::transit::disruptions::overlay`].

use actix_web::{HttpResponse, delete, get, post, put, web};
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::shared::config::AppConfig;
use crate::transit::disruptions::model::{Cause, Disruption, DisruptionInput};
use crate::transit::disruptions::overlay;
use crate::transit::disruptions::store::{DisruptionStore, StoreError};
use crate::transit::raptor::RaptorData;

/// Query parameters for `GET /api/disruptions`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct DisruptionsQuery {
    /// Keep only the disruptions in force at this local date-time
    /// (`YYYY-MM-DDTHH:MM:SS`). `now` is accepted as a shorthand.
    pub active_at: Option<String>,
    /// Keep only this kind of scope: `stop`, `line` or `line_section`.
    pub scope: Option<String>,
}

/// Response for `GET /api/disruptions`.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisruptionsResponse {
    pub disruptions: Vec<Disruption>,
}

/// List disruptions, newest first.
#[utoipa::path(
    get,
    path = "/api/disruptions",
    params(DisruptionsQuery),
    responses(
        (status = 200, description = "Disruption catalog", body = DisruptionsResponse),
        (status = 400, description = "Invalid filter"),
    ),
    tag = "Disruptions"
)]
#[get("/api/disruptions")]
pub async fn get_disruptions(
    query: web::Query<DisruptionsQuery>,
    store: web::Data<DisruptionStore>,
) -> HttpResponse {
    let active_at = match parse_active_at(query.active_at.as_deref()) {
        Ok(instant) => instant,
        Err(response) => return response,
    };

    let catalog = store.snapshot();
    let mut disruptions: Vec<Disruption> = catalog
        .disruptions
        .iter()
        .filter(|d| active_at.is_none_or(|instant| d.period.covers(instant)))
        .filter(|d| {
            query
                .scope
                .as_deref()
                .is_none_or(|kind| d.scope.kind() == kind)
        })
        .cloned()
        .collect();

    // Newest first: the back office lists what was just entered at the top.
    disruptions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    HttpResponse::Ok().json(DisruptionsResponse { disruptions })
}

/// A stop a disruption closes, with what the map needs to draw it.
#[derive(Debug, Serialize, ToSchema)]
pub struct BlockedStop {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// One blocking disruption, resolved to coordinates.
#[derive(Debug, Serialize, ToSchema)]
pub struct BlockedDisruption {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
    pub cause: Cause,
    /// What the disruption names: "stop", "line" or "line_section".
    pub scope: String,
    pub starts_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    /// Stops it closes. Empty for a line or section closure.
    pub stops: Vec<BlockedStop>,
    /// Rides it cuts, as `[[lat, lon], [lat, lon]]` pairs in WGS84.
    pub segments: Vec<[[f64; 2]; 2]>,
}

/// Response of `GET /api/disruptions/active`.
#[derive(Debug, Serialize, ToSchema)]
pub struct BlockedDisruptionsResponse {
    /// Instant the catalog was resolved at, `YYYY-MM-DDTHH:MM:SS` local.
    pub resolved_at: String,
    pub disruptions: Vec<BlockedDisruption>,
}

/// Blocking disruptions in force right now, resolved to map geometry.
///
/// Separate from `GET /api/disruptions` because the two answer different
/// questions: the catalog lists what an operator entered, in operator terms;
/// this lists what is *currently removed from the network*, in coordinates.
/// Informational disruptions are excluded — they remove nothing.
#[utoipa::path(
    get,
    path = "/api/disruptions/active",
    responses((status = 200, description = "Blocking disruptions in force", body = BlockedDisruptionsResponse)),
    tag = "Disruptions"
)]
#[get("/api/disruptions/active")]
pub async fn get_active_disruptions(
    store: web::Data<DisruptionStore>,
    shared: web::Data<ArcSwap<RaptorData>>,
) -> HttpResponse {
    let data = shared.load();
    let now = chrono::Local::now().naive_local();
    let index = overlay::resolve(&data, &store.snapshot(), now);

    let disruptions = overlay::blocked_geometry(&index, &data)
        .into_iter()
        .filter_map(|(entry, geometry)| {
            let disruption = index.entry(entry)?;
            Some(BlockedDisruption {
                id: disruption.id.clone(),
                title: disruption.title.clone(),
                message: disruption.message.clone(),
                cause: disruption.cause,
                scope: disruption.scope.kind().to_string(),
                starts_at: format_local(disruption.period.starts_at),
                ends_at: disruption.period.ends_at.map(format_local),
                stops: geometry
                    .stops
                    .iter()
                    .filter_map(|&idx| data.stops.get(idx))
                    // Station nodes carry no patterns and often no usable
                    // coordinates; the platforms under them are what riders see.
                    .filter(|stop| stop.stop_lat != 0.0 || stop.stop_lon != 0.0)
                    .map(|stop| BlockedStop {
                        id: stop.stop_id.clone(),
                        name: stop.stop_name.clone(),
                        lat: stop.stop_lat,
                        lon: stop.stop_lon,
                    })
                    .collect(),
                segments: geometry
                    .edges
                    .iter()
                    .filter_map(|&(from, to)| {
                        let from = data.stops.get(from)?;
                        let to = data.stops.get(to)?;
                        Some([[from.stop_lat, from.stop_lon], [to.stop_lat, to.stop_lon]])
                    })
                    .collect(),
            })
        })
        .collect();

    HttpResponse::Ok().json(BlockedDisruptionsResponse {
        resolved_at: format_local(now),
        disruptions,
    })
}

/// Render a local date-time the way the API accepts it back.
fn format_local(instant: chrono::NaiveDateTime) -> String {
    instant.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Fetch one disruption by id.
#[utoipa::path(
    get,
    path = "/api/disruptions/{id}",
    params(("id" = String, Path, description = "Disruption id")),
    responses(
        (status = 200, description = "The disruption", body = Disruption),
        (status = 404, description = "No such disruption"),
    ),
    tag = "Disruptions"
)]
#[get("/api/disruptions/{id}")]
pub async fn get_disruption(
    path: web::Path<String>,
    store: web::Data<DisruptionStore>,
) -> HttpResponse {
    match store.snapshot().get(&path.into_inner()) {
        Some(disruption) => HttpResponse::Ok().json(disruption),
        None => not_found(),
    }
}

/// Create a disruption.
#[utoipa::path(
    post,
    path = "/api/disruptions",
    request_body = DisruptionInput,
    responses(
        (status = 201, description = "Created", body = Disruption),
        (status = 400, description = "Invalid payload"),
        (status = 401, description = "Invalid or missing API key"),
        (status = 403, description = "Writes disabled (no api_key configured)"),
        (status = 500, description = "Could not persist the catalog"),
    ),
    security(("api_key" = [])),
    tag = "Disruptions"
)]
#[post("/api/disruptions")]
pub async fn post_disruption(
    req: actix_web::HttpRequest,
    body: web::Json<DisruptionInput>,
    store: web::Data<DisruptionStore>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    if let Err(response) = authorize(&req, &config) {
        return response;
    }
    match store.create(body.into_inner()) {
        Ok(disruption) => HttpResponse::Created().json(disruption),
        Err(e) => store_error(e),
    }
}

/// Replace the mutable fields of a disruption.
#[utoipa::path(
    put,
    path = "/api/disruptions/{id}",
    params(("id" = String, Path, description = "Disruption id")),
    request_body = DisruptionInput,
    responses(
        (status = 200, description = "Updated", body = Disruption),
        (status = 400, description = "Invalid payload"),
        (status = 401, description = "Invalid or missing API key"),
        (status = 403, description = "Writes disabled (no api_key configured)"),
        (status = 404, description = "No such disruption"),
        (status = 500, description = "Could not persist the catalog"),
    ),
    security(("api_key" = [])),
    tag = "Disruptions"
)]
#[put("/api/disruptions/{id}")]
pub async fn put_disruption(
    req: actix_web::HttpRequest,
    path: web::Path<String>,
    body: web::Json<DisruptionInput>,
    store: web::Data<DisruptionStore>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    if let Err(response) = authorize(&req, &config) {
        return response;
    }
    match store.update(&path.into_inner(), body.into_inner()) {
        Ok(disruption) => HttpResponse::Ok().json(disruption),
        Err(e) => store_error(e),
    }
}

/// Delete a disruption.
#[utoipa::path(
    delete,
    path = "/api/disruptions/{id}",
    params(("id" = String, Path, description = "Disruption id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Invalid or missing API key"),
        (status = 403, description = "Writes disabled (no api_key configured)"),
        (status = 404, description = "No such disruption"),
        (status = 500, description = "Could not persist the catalog"),
    ),
    security(("api_key" = [])),
    tag = "Disruptions"
)]
#[delete("/api/disruptions/{id}")]
pub async fn delete_disruption(
    req: actix_web::HttpRequest,
    path: web::Path<String>,
    store: web::Data<DisruptionStore>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    if let Err(response) = authorize(&req, &config) {
        return response;
    }
    match store.delete(&path.into_inner()) {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => store_error(e),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Reject the request unless it carries the configured `X-Api-Key`.
fn authorize(req: &actix_web::HttpRequest, config: &AppConfig) -> Result<(), HttpResponse> {
    let expected = &config.server.api_key;
    if expected.is_empty() {
        return Err(HttpResponse::Forbidden().json(serde_json::json!({
            "error": {
                "id": "disabled",
                "message": "Disruption writes are disabled (no api_key configured)"
            }
        })));
    }
    let provided = req
        .headers()
        .get("X-Api-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected {
        return Err(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": { "id": "unauthorized", "message": "Invalid or missing X-Api-Key header" }
        })));
    }
    Ok(())
}

/// Parse the `active_at` filter. `None` means "no filter", not "now".
fn parse_active_at(raw: Option<&str>) -> Result<Option<chrono::NaiveDateTime>, HttpResponse> {
    match raw {
        None => Ok(None),
        Some("now") => Ok(Some(chrono::Local::now().naive_local())),
        Some(text) => chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
            .map(Some)
            .map_err(|_| {
                bad_request(
                    "bad_request",
                    "Invalid active_at. Use YYYY-MM-DDTHH:MM:SS or 'now'",
                )
            }),
    }
}

fn store_error(error: StoreError) -> HttpResponse {
    match error {
        StoreError::NotFound => not_found(),
        StoreError::Invalid(reason) => bad_request("bad_request", &reason),
        StoreError::Io(reason) => {
            tracing::error!("Disruption catalog write failed: {reason}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "internal_error", "message": "Could not persist the catalog" }
            }))
        }
    }
}

fn not_found() -> HttpResponse {
    HttpResponse::NotFound().json(serde_json::json!({
        "error": { "id": "unknown_object", "message": "No such disruption" }
    }))
}

fn bad_request(id: &str, message: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(serde_json::json!({
        "error": { "id": id, "message": message }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;

    const KEY: &str = "secret";

    fn config_with_key(key: &str) -> web::Data<AppConfig> {
        let mut cfg = AppConfig::default();
        cfg.server.api_key = key.to_string();
        web::Data::new(cfg)
    }

    fn store(dir: &tempfile::TempDir) -> web::Data<DisruptionStore> {
        web::Data::new(DisruptionStore::load(&dir.path().join("disruptions.json")))
    }

    fn payload() -> serde_json::Value {
        serde_json::json!({
            "title": "Travaux — station fermée",
            "message": "Réouverture prévue le 8 septembre",
            "cause": "works",
            "severity": "blocking",
            "scope": { "type": "stop", "stop_id": "S2" },
            "starts_at": "2026-09-01T22:00:00"
        })
    }

    /// Registers the routes in the same order as `main.rs`: `/active` must
    /// come before `/{id}`, which would otherwise capture it.
    macro_rules! app_with {
        ($store:expr, $config:expr) => {
            test::init_service(
                actix_web::App::new()
                    .app_data($store)
                    .app_data($config)
                    .app_data(web::Data::new(ArcSwap::from(std::sync::Arc::new(
                        crate::transit::raptor::test_support::build_test_data(),
                    ))))
                    .service(get_disruptions)
                    .service(post_disruption)
                    .service(get_active_disruptions)
                    .service(get_disruption)
                    .service(put_disruption)
                    .service(delete_disruption),
            )
            .await
        };
    }

    #[actix_web::test]
    async fn post_then_get_round_trips_a_disruption() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", KEY))
            .set_json(payload())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let created: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(created["id"], "d1");
        assert_eq!(created["scope"]["type"], "stop");
        assert_eq!(created["severity"], "blocking");
        assert!(created["ends_at"].is_null());

        let req = test::TestRequest::get()
            .uri("/api/disruptions/d1")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let fetched: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(fetched["title"], "Travaux — station fermée");
    }

    #[actix_web::test]
    async fn writes_require_the_api_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .set_json(payload())
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 401);

        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", "wrong"))
            .set_json(payload())
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 401);
    }

    #[actix_web::test]
    async fn writes_are_disabled_when_no_api_key_is_configured() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(""));

        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", "anything"))
            .set_json(payload())
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 403);
    }

    #[actix_web::test]
    async fn reads_stay_open_without_a_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::get()
            .uri("/api/disruptions")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["disruptions"].as_array().expect("array").len(), 0);
    }

    #[actix_web::test]
    async fn an_invalid_payload_is_rejected_with_the_reason() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let mut bad = payload();
        bad["ends_at"] = serde_json::json!("2026-08-01T00:00:00");
        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", KEY))
            .set_json(bad)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(
            body["error"]["message"]
                .as_str()
                .expect("message")
                .contains("ends_at")
        );
    }

    #[actix_web::test]
    async fn put_updates_and_delete_removes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", KEY))
            .set_json(payload())
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 201);

        let mut changed = payload();
        changed["severity"] = serde_json::json!("info");
        changed["title"] = serde_json::json!("Quai modifié");
        let req = test::TestRequest::put()
            .uri("/api/disruptions/d1")
            .insert_header(("X-Api-Key", KEY))
            .set_json(changed)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let updated: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(updated["severity"], "info");
        assert_eq!(updated["title"], "Quai modifié");

        let req = test::TestRequest::delete()
            .uri("/api/disruptions/d1")
            .insert_header(("X-Api-Key", KEY))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 204);

        let req = test::TestRequest::get()
            .uri("/api/disruptions/d1")
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 404);
    }

    #[actix_web::test]
    async fn an_unknown_id_is_reported_as_not_found() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::put()
            .uri("/api/disruptions/d404")
            .insert_header(("X-Api-Key", KEY))
            .set_json(payload())
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 404);

        let req = test::TestRequest::delete()
            .uri("/api/disruptions/d404")
            .insert_header(("X-Api-Key", KEY))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 404);
    }

    #[actix_web::test]
    async fn active_at_and_scope_filter_the_listing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        // A stop closure that ends before September.
        let mut past = payload();
        past["starts_at"] = serde_json::json!("2026-08-01T00:00:00");
        past["ends_at"] = serde_json::json!("2026-08-02T00:00:00");
        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", KEY))
            .set_json(past)
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 201);

        // An open-ended line closure.
        let mut line = payload();
        line["scope"] = serde_json::json!({ "type": "line", "route_id": "R1" });
        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", KEY))
            .set_json(line)
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 201);

        let req = test::TestRequest::get()
            .uri("/api/disruptions?active_at=2026-09-02T10:00:00")
            .to_request();
        let body: serde_json::Value =
            test::read_body_json(test::call_service(&app, req).await).await;
        let listed = body["disruptions"].as_array().expect("array");
        assert_eq!(listed.len(), 1, "only the open-ended one is still in force");
        assert_eq!(listed[0]["scope"]["type"], "line");

        let req = test::TestRequest::get()
            .uri("/api/disruptions?scope=stop")
            .to_request();
        let body: serde_json::Value =
            test::read_body_json(test::call_service(&app, req).await).await;
        assert_eq!(body["disruptions"].as_array().expect("array").len(), 1);
    }

    #[actix_web::test]
    async fn an_invalid_active_at_is_rejected() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::get()
            .uri("/api/disruptions?active_at=hier")
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 400);
    }

    #[actix_web::test]
    async fn active_at_accepts_now() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::get()
            .uri("/api/disruptions?active_at=now")
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);
    }

    // -----------------------------------------------------------------------
    // GET /api/disruptions/active
    // -----------------------------------------------------------------------

    use crate::transit::disruptions::model::{Period, Scope, Severity};

    /// A disruption in force for the whole of 2026.
    fn in_force(id: &str, scope: Scope, severity: Severity) -> Disruption {
        let start =
            chrono::NaiveDateTime::parse_from_str("2020-01-01T00:00:00", "%Y-%m-%dT%H:%M:%S")
                .expect("valid test time");
        Disruption {
            id: id.to_string(),
            title: "Travaux".to_string(),
            message: "Station fermée".to_string(),
            cause: Cause::Works,
            severity,
            scope,
            period: Period {
                starts_at: start,
                ends_at: None,
            },
            created_at: start,
            updated_at: start,
        }
    }

    async fn active_with(disruptions: Vec<Disruption>) -> serde_json::Value {
        let app = app_with!(
            web::Data::new(DisruptionStore::for_tests(disruptions)),
            config_with_key(KEY)
        );
        let req = test::TestRequest::get()
            .uri("/api/disruptions/active")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        test::read_body_json(resp).await
    }

    #[actix_web::test]
    async fn active_resolves_a_closed_stop_to_coordinates() {
        let body = active_with(vec![in_force(
            "d1",
            Scope::Stop {
                stop_id: "S2".into(),
            },
            Severity::Blocking,
        )])
        .await;

        let listed = body["disruptions"].as_array().expect("disruptions");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["id"], "d1");
        assert_eq!(listed[0]["scope"], "stop");

        let stops = listed[0]["stops"].as_array().expect("stops");
        assert_eq!(stops.len(), 1);
        assert_eq!(stops[0]["id"], "S2");
        assert_eq!(stops[0]["lat"], 48.844);
        assert_eq!(stops[0]["lon"], 2.373);
    }

    #[actix_web::test]
    async fn active_expands_a_closed_platform_to_its_station() {
        // S1 and S4 both hang off parent station P1.
        let body = active_with(vec![in_force(
            "d1",
            Scope::Stop {
                stop_id: "S1".into(),
            },
            Severity::Blocking,
        )])
        .await;

        let ids: Vec<&str> = body["disruptions"][0]["stops"]
            .as_array()
            .expect("stops")
            .iter()
            .filter_map(|s| s["id"].as_str())
            .collect();
        assert!(ids.contains(&"S1"));
        assert!(
            ids.contains(&"S4"),
            "the sibling platform must be closed too"
        );
    }

    #[actix_web::test]
    async fn active_returns_segments_for_a_closed_line() {
        let body = active_with(vec![in_force(
            "d1",
            Scope::Line {
                route_id: "R1".into(),
            },
            Severity::Blocking,
        )])
        .await;

        let disruption = &body["disruptions"][0];
        assert_eq!(disruption["scope"], "line");
        assert!(
            disruption["stops"].as_array().expect("stops").is_empty(),
            "a line closure names no single stop"
        );

        let segments = disruption["segments"].as_array().expect("segments");
        assert!(!segments.is_empty());
        // Each segment is a [[lat, lon], [lat, lon]] pair.
        assert_eq!(segments[0].as_array().expect("pair").len(), 2);
        assert_eq!(segments[0][0].as_array().expect("point").len(), 2);
    }

    #[actix_web::test]
    async fn active_skips_informational_disruptions() {
        let body = active_with(vec![in_force(
            "d1",
            Scope::Stop {
                stop_id: "S2".into(),
            },
            Severity::Info,
        )])
        .await;
        assert!(
            body["disruptions"]
                .as_array()
                .expect("disruptions")
                .is_empty()
        );
    }

    #[actix_web::test]
    async fn active_skips_a_disruption_that_is_over() {
        let mut past = in_force(
            "d1",
            Scope::Stop {
                stop_id: "S2".into(),
            },
            Severity::Blocking,
        );
        past.period.ends_at = Some(
            chrono::NaiveDateTime::parse_from_str("2020-01-02T00:00:00", "%Y-%m-%dT%H:%M:%S")
                .expect("valid test time"),
        );

        let body = active_with(vec![past]).await;
        assert!(
            body["disruptions"]
                .as_array()
                .expect("disruptions")
                .is_empty()
        );
    }

    #[actix_web::test]
    async fn active_skips_an_identifier_absent_from_the_dataset() {
        let body = active_with(vec![in_force(
            "d1",
            Scope::Line {
                route_id: "does-not-exist".into(),
            },
            Severity::Blocking,
        )])
        .await;
        assert!(
            body["disruptions"]
                .as_array()
                .expect("disruptions")
                .is_empty(),
            "nothing is removed, so nothing is drawn"
        );
    }

    #[actix_web::test]
    async fn the_active_route_does_not_shadow_an_id_lookup() {
        let dir = tempfile::tempdir().expect("temp dir");
        let app = app_with!(store(&dir), config_with_key(KEY));

        let req = test::TestRequest::post()
            .uri("/api/disruptions")
            .insert_header(("X-Api-Key", KEY))
            .set_json(payload())
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 201);

        let req = test::TestRequest::get()
            .uri("/api/disruptions/d1")
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);

        let req = test::TestRequest::get()
            .uri("/api/disruptions/active")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(
            body["resolved_at"].is_string(),
            "the active route answered, not the id lookup"
        );
    }
}
