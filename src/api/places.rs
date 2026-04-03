//! Stop name autocomplete endpoint.

use actix_web::{get, web, HttpResponse};
use arc_swap::ArcSwap;
use serde::Deserialize;

use crate::raptor::RaptorData;

/// Query parameters for `GET /api/places`.
#[derive(Debug, Deserialize)]
pub struct PlacesQuery {
    /// Search query string (min 2 characters).
    pub q: Option<String>,
    /// Maximum number of results (default 10, max 50).
    pub limit: Option<usize>,
}

/// Autocomplete stop search. Returns up to `limit` matching stops
/// ranked by relevance (exact > prefix > word-prefix > substring).
#[get("/api/places")]
pub async fn get_places(
    query: web::Query<PlacesQuery>,
    shared: web::Data<ArcSwap<RaptorData>>,
) -> HttpResponse {
    let raptor_data = shared.load();
    let q = query.q.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(10).min(50);

    let results = raptor_data.search_stops(q, limit);

    let places: Vec<serde_json::Value> = results
        .iter()
        .map(|(idx, name, id)| {
            let stop = &raptor_data.stops[*idx];
            serde_json::json!({
                "id": id,
                "name": name,
                "coord": {
                    "lon": stop.stop_lon,
                    "lat": stop.stop_lat,
                }
            })
        })
        .collect();

    HttpResponse::Ok().json(serde_json::json!({ "places": places }))
}
