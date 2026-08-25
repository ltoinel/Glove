//! Line catalogue for the back office.
//!
//! Exists so the disruption screen can offer a line picker: an operator
//! declaring works on "ligne 4" cannot be asked to type `IDFM:C01374`. Reads
//! straight from the RAPTOR index, which already holds every route's display
//! metadata.

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::shared::text::normalize;
use crate::transit::raptor::RaptorData;

/// How many lines a single response may carry.
const MAX_LIMIT: usize = 200;
const DEFAULT_LIMIT: usize = 50;

/// Query parameters for `GET /api/lines`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct LinesQuery {
    /// Case- and accent-insensitive filter on the short or long name.
    pub q: Option<String>,
    /// Maximum number of results (default 50, max 200).
    pub limit: Option<usize>,
}

/// One line, as the picker displays it.
#[derive(Debug, Serialize, ToSchema)]
pub struct Line {
    pub id: String,
    /// Short name as printed on the vehicle ("4", "RER A").
    pub short_name: String,
    pub long_name: String,
    /// Commercial mode: "metro", "bus", "rail"…
    pub mode: String,
    pub color: String,
    pub text_color: String,
}

/// Response for `GET /api/lines`.
#[derive(Debug, Serialize, ToSchema)]
pub struct LinesResponse {
    pub lines: Vec<Line>,
}

/// List the lines of the loaded dataset, optionally filtered by name.
#[utoipa::path(
    get,
    path = "/api/lines",
    params(LinesQuery),
    responses((status = 200, description = "Matching lines", body = LinesResponse)),
    tag = "Lines"
)]
#[get("/api/lines")]
pub async fn get_lines(
    query: web::Query<LinesQuery>,
    shared: web::Data<ArcSwap<RaptorData>>,
) -> HttpResponse {
    let data = shared.load();
    let needle = query.q.as_deref().map(normalize).unwrap_or_default();
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    let mut lines: Vec<Line> = data
        .routes
        .values()
        .filter(|route| {
            needle.is_empty()
                || normalize(&route.route_short_name).contains(&needle)
                || normalize(&route.route_long_name).contains(&needle)
        })
        .map(|route| Line {
            id: route.route_id.clone(),
            short_name: route.route_short_name.clone(),
            long_name: route.route_long_name.clone(),
            mode: super::journeys::public_transport::route_type_to_mode(route.route_type)
                .to_string(),
            color: route.route_color.clone(),
            text_color: route.route_text_color.clone(),
        })
        .collect();

    // Short names first so "1" precedes "10": a plain string sort would not.
    lines.sort_by(|a, b| {
        natural_key(&a.short_name)
            .cmp(&natural_key(&b.short_name))
            .then_with(|| a.short_name.cmp(&b.short_name))
    });
    lines.truncate(limit);

    HttpResponse::Ok().json(LinesResponse { lines })
}

/// Sort key placing purely numeric names in numeric order, before the rest.
fn natural_key(name: &str) -> (u8, u32) {
    match name.parse::<u32>() {
        Ok(number) => (0, number),
        Err(_) => (1, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transit::gtfs;
    use actix_web::test;
    use rustc_hash::FxHashMap;
    use std::sync::Arc;

    fn route(id: &str, short: &str, long: &str, route_type: u16) -> gtfs::Route {
        gtfs::Route {
            route_id: id.to_string(),
            agency_id: "A1".to_string(),
            route_short_name: short.to_string(),
            route_long_name: long.to_string(),
            route_type,
            route_color: "FFCD00".to_string(),
            route_text_color: "000000".to_string(),
        }
    }

    fn data_with_routes() -> Arc<RaptorData> {
        let mut data = crate::transit::raptor::test_support::build_test_data();
        let mut routes = FxHashMap::default();
        routes.insert("R10".to_string(), route("R10", "10", "Boulogne", 1));
        routes.insert(
            "R1".to_string(),
            route("R1", "1", "Château de Vincennes", 1),
        );
        routes.insert("RA".to_string(), route("RA", "RER A", "Saint-Germain", 2));
        data.routes = routes;
        Arc::new(data)
    }

    async fn call(uri: &str) -> serde_json::Value {
        let app = test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(ArcSwap::from(data_with_routes())))
                .service(get_lines),
        )
        .await;
        let req = test::TestRequest::get().uri(uri).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        test::read_body_json(resp).await
    }

    #[actix_web::test]
    async fn lists_every_line_sorted_numerically_first() {
        let body = call("/api/lines").await;
        let lines = body["lines"].as_array().expect("lines");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["short_name"], "1");
        assert_eq!(lines[1]["short_name"], "10");
        assert_eq!(lines[2]["short_name"], "RER A");
    }

    #[actix_web::test]
    async fn filters_on_either_name_ignoring_accents() {
        let body = call("/api/lines?q=chateau").await;
        let lines = body["lines"].as_array().expect("lines");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["id"], "R1");

        let body = call("/api/lines?q=rer").await;
        assert_eq!(body["lines"].as_array().expect("lines").len(), 1);
    }

    #[actix_web::test]
    async fn reports_the_commercial_mode_and_colours() {
        let body = call("/api/lines?q=RER").await;
        let line = &body["lines"][0];
        assert_eq!(line["mode"], "rail");
        assert_eq!(line["color"], "FFCD00");
        assert_eq!(line["text_color"], "000000");
    }

    #[actix_web::test]
    async fn limit_is_capped() {
        let body = call("/api/lines?limit=1").await;
        assert_eq!(body["lines"].as_array().expect("lines").len(), 1);
    }
}
