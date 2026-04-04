//! Stop and address name autocomplete endpoint.

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

use crate::ban::BanData;
use crate::raptor::RaptorData;

/// Query parameters for `GET /api/places`.
#[derive(Debug, Deserialize, IntoParams)]
pub struct PlacesQuery {
    /// Search query string (min 2 characters).
    pub q: Option<String>,
    /// Maximum number of results (default 10, max 50).
    pub limit: Option<usize>,
}

/// Response for `GET /api/places`.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlacesResponse {
    pub places: Vec<PlaceResult>,
}

/// A single place result from autocomplete search.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaceResult {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub place_type: String,
    pub coord: super::Coord,
}

/// Autocomplete search across GTFS stops and BAN addresses.
/// Returns up to `limit` matching results ranked by relevance.
#[utoipa::path(
    get,
    path = "/api/places",
    params(PlacesQuery),
    responses(
        (status = 200, description = "Matching stops and addresses", body = PlacesResponse),
    ),
    tag = "Places"
)]
#[get("/api/places")]
pub async fn get_places(
    query: web::Query<PlacesQuery>,
    shared: web::Data<ArcSwap<RaptorData>>,
    ban: web::Data<Arc<BanData>>,
) -> HttpResponse {
    let raptor_data = shared.load();
    let q = query.q.as_deref().unwrap_or("");
    let limit = query.limit.unwrap_or(10).min(50);

    if q.len() < 2 {
        return HttpResponse::Ok().json(serde_json::json!({ "places": [] }));
    }

    // Search both sources
    let stop_results = raptor_data.search_stops(q, limit);
    let ban_results = ban.search(q, limit);

    // Interleave: stops first (higher priority), then addresses, up to limit
    let mut places: Vec<serde_json::Value> = Vec::with_capacity(limit);

    let mut si = 0;
    let mut bi = 0;
    while places.len() < limit && (si < stop_results.len() || bi < ban_results.len()) {
        // Alternate: add one stop, then one address
        if si < stop_results.len() && places.len() < limit {
            let (idx, name, _id) = &stop_results[si];
            let stop = &raptor_data.stops[*idx];
            places.push(serde_json::json!({
                "id": stop.stop_id,
                "name": name,
                "type": "stop",
                "coord": {
                    "lon": stop.stop_lon,
                    "lat": stop.stop_lat,
                }
            }));
            si += 1;
        }
        if bi < ban_results.len() && places.len() < limit {
            let entry = &ban_results[bi];
            // Use lon;lat as ID so resolve_stop() can find the nearest stop
            let id = format!("{};{}", entry.lon, entry.lat);
            places.push(serde_json::json!({
                "id": id,
                "name": entry.label,
                "type": "address",
                "coord": {
                    "lon": entry.lon,
                    "lat": entry.lat,
                }
            }));
            bi += 1;
        }
    }

    HttpResponse::Ok().json(serde_json::json!({ "places": places }))
}
