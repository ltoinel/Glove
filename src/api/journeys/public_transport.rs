//! Journey planning endpoint (Navitia-compatible).
//!
//! Runs RAPTOR iteratively with pattern exclusion to produce diverse
//! route alternatives, sorted by duration and tagged with quality labels.

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwap;
use chrono::Timelike;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use std::sync::Arc;

use crate::config::AppConfig;
use crate::raptor::{self, RaptorData, SectionType};

use super::valhalla;
use crate::api::{Place, Section, StopDateTime, make_place, make_stop_point};

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/journeys`.
///
/// Mirrors the Navitia `/journeys` API parameters. Most fields are optional
/// and fall back to values from [`AppConfig`].
#[derive(Debug, Deserialize, IntoParams)]
#[allow(dead_code)]
pub struct JourneysQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub datetime: Option<String>,
    pub datetime_represents: Option<DatetimeRepresents>,

    pub max_nb_transfers: Option<i32>,
    pub min_nb_transfers: Option<i32>,

    #[serde(rename = "first_section_mode[]", default)]
    pub first_section_mode: Vec<String>,
    #[serde(rename = "last_section_mode[]", default)]
    pub last_section_mode: Vec<String>,

    pub max_duration_to_pt: Option<i32>,
    pub max_walking_duration_to_pt: Option<i32>,
    pub max_bike_duration_to_pt: Option<i32>,
    pub max_bss_duration_to_pt: Option<i32>,
    pub max_car_duration_to_pt: Option<i32>,
    pub max_ridesharing_duration_to_pt: Option<i32>,

    pub walking_speed: Option<f64>,
    pub bike_speed: Option<f64>,
    pub bss_speed: Option<f64>,
    pub car_speed: Option<f64>,
    pub ridesharing_speed: Option<f64>,
    pub taxi_speed: Option<f64>,

    #[serde(rename = "forbidden_uris[]", default)]
    pub forbidden_uris: Vec<String>,
    #[serde(rename = "allowed_id[]", default)]
    pub allowed_id: Vec<String>,

    pub disruption_active: Option<bool>,
    pub data_freshness: Option<DataFreshness>,

    pub max_duration: Option<i32>,
    pub wheelchair: Option<bool>,
    pub traveler_type: Option<String>,
    pub direct_path: Option<String>,

    pub free_radius_from: Option<i32>,
    pub free_radius_to: Option<i32>,

    /// Comma-separated commercial modes to exclude (e.g. "metro,bus,rail").
    pub forbidden_modes: Option<String>,

    pub count: Option<i32>,
    pub min_nb_journeys: Option<i32>,
    pub max_nb_journeys: Option<i32>,

    pub is_journey_schedules: Option<bool>,
    pub timeframe_duration: Option<i32>,

    pub max_taxi_direct_path_duration: Option<i32>,
    pub max_walking_direct_path_duration: Option<i32>,
    pub max_car_direct_path_duration: Option<i32>,
    pub max_ridesharing_direct_path_duration: Option<i32>,
    pub max_bss_direct_path_duration: Option<i32>,
    pub max_bike_direct_path_duration: Option<i32>,

    #[serde(rename = "add_poi_infos[]", default)]
    pub add_poi_infos: Vec<String>,
    pub bss_stands: Option<bool>,
    pub equipment_details: Option<bool>,
    pub language: Option<String>,

    /// Include turn-by-turn maneuvers in walking/transfer sections (default: false).
    pub maneuvers: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatetimeRepresents {
    Departure,
    Arrival,
}

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataFreshness {
    BaseSchedule,
    AdaptedSchedule,
    Realtime,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level response for `GET /api/journeys`.
#[derive(Debug, Serialize, ToSchema)]
pub struct JourneysResponse {
    pub journeys: Vec<Journey>,
}

/// A complete journey from origin to destination.
#[derive(Debug, Serialize, ToSchema)]
pub struct Journey {
    pub departure_date_time: String,
    pub arrival_date_time: String,
    pub duration: u32,
    pub nb_transfers: u32,
    /// Quality tags: "fastest", "least_transfers", "least_walking".
    pub tags: Vec<String>,
    pub sections: Vec<Section>,
}

/// Display information for a public transport section.
#[derive(Debug, Serialize, ToSchema)]
pub struct DisplayInfo {
    pub network: String,
    pub direction: String,
    pub commercial_mode: String,
    pub label: String,
    pub color: String,
    pub text_color: String,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Compute journey alternatives between two stops.
///
/// Runs RAPTOR iteratively, each time excluding the route patterns used by
/// previously found journeys, to produce diverse alternatives sorted by
/// duration (fastest first).
#[utoipa::path(
    get,
    path = "/api/journeys/public_transport",
    params(JourneysQuery),
    responses(
        (status = 200, description = "Journey alternatives", body = JourneysResponse),
        (status = 400, description = "Invalid parameters"),
    ),
    tag = "Journeys"
)]
#[get("/api/journeys/public_transport")]
pub async fn get_journeys(
    query: web::Query<JourneysQuery>,
    shared: web::Data<ArcSwap<RaptorData>>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    let raptor_data = shared.load();

    let from_str = match &query.from {
        Some(f) => f.as_str(),
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": { "id": "bad_request", "message": "'from' parameter is required" }
            }));
        }
    };

    let to_str = match &query.to {
        Some(t) => t.as_str(),
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": { "id": "bad_request", "message": "'to' parameter is required" }
            }));
        }
    };

    let from_coord = parse_coord(from_str);
    let to_coord = parse_coord(to_str);

    let max_dist = config.routing.max_nearest_stop_distance;
    let walking_speed = query.walking_speed.unwrap_or(5.0);

    let sources = resolve_stops(&raptor_data, from_str, from_coord, max_dist, walking_speed);
    if sources.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": { "id": "unknown_object", "message": format!("No stop found within {} m of {from_str}", max_dist) }
        }));
    }

    let targets = resolve_stops(&raptor_data, to_str, to_coord, max_dist, walking_speed);
    if targets.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": { "id": "unknown_object", "message": format!("No stop found within {} m of {to_str}", max_dist) }
        }));
    }

    let (date, departure_time) = match &query.datetime {
        Some(dt) => match raptor::parse_datetime(dt) {
            Some(parsed) => parsed,
            None => {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "error": { "id": "bad_request", "message": "Invalid datetime format. Use YYYYMMDDTHHmmss" }
                }));
            }
        },
        None => {
            let now = chrono::Local::now();
            let today = now.format("%Y%m%d").to_string();
            let secs = now.hour() * 3600 + now.minute() * 60 + now.second();
            (today, secs)
        }
    };

    let max_transfers = query
        .max_nb_transfers
        .map(|n| n.max(0) as usize)
        .unwrap_or(config.routing.max_transfers);

    let max_duration = query
        .max_duration
        .map(|n| n.max(0) as u32)
        .unwrap_or(config.routing.max_duration);

    let requested = query
        .count
        .or(query.max_nb_journeys)
        .map(|n| (n.max(1) as usize).min(config.routing.max_journeys))
        .unwrap_or(config.routing.max_journeys);

    // Pre-compute active services. For early morning queries (before 4h),
    // also include yesterday's services with +86400s offset, because GTFS
    // encodes after-midnight trips on the previous day (e.g. 25:30:00).
    const EARLY_MORNING_THRESHOLD: u32 = 4 * 3600; // 04:00

    let (effective_date, effective_departure) = if departure_time < EARLY_MORNING_THRESHOLD {
        // Use yesterday's date with time shifted by +24h so RAPTOR matches
        // after-midnight trips (coded as >86400 on the previous day)
        let yesterday = {
            let y: i32 = date.get(0..4).and_then(|s| s.parse().ok()).unwrap_or(2026);
            let m: u32 = date.get(4..6).and_then(|s| s.parse().ok()).unwrap_or(1);
            let d: u32 = date.get(6..8).and_then(|s| s.parse().ok()).unwrap_or(1);
            chrono::NaiveDate::from_ymd_opt(y, m, d)
                .and_then(|dt| dt.pred_opt())
                .map(|dt| dt.format("%Y%m%d").to_string())
                .unwrap_or_else(|| date.clone())
        };
        (yesterday, departure_time + 86400)
    } else {
        (date.clone(), departure_time)
    };

    let active = raptor_data.active_services(&effective_date);

    let mode_excluded =
        compute_mode_exclusions(&raptor_data, query.forbidden_modes.as_deref().unwrap_or(""));

    // Iterative search with pattern exclusion for route diversity
    let mut journeys: Vec<Journey> = Vec::new();
    let mut excluded_patterns = mode_excluded.clone();

    for _ in 0..requested {
        if journeys.len() >= requested {
            break;
        }

        let result = raptor::raptor_query(
            &raptor_data,
            &sources,
            effective_departure,
            &active,
            max_transfers,
            &excluded_patterns,
        );

        let section_sets = raptor::reconstruct_journeys(&raptor_data, &result, &targets);
        if section_sets.is_empty() {
            break;
        }

        // Exploit all Pareto-optimal journeys from this RAPTOR run
        let prev_count = journeys.len();
        for sections in &section_sets {
            let journey = build_journey(&raptor_data, sections, &effective_date);

            if journey.duration > max_duration {
                continue;
            }

            let dominated = journeys
                .iter()
                .any(|existing| journey_sections_equal(existing, &journey));

            if !dominated {
                journeys.push(journey);
            }

            excluded_patterns.extend(raptor::used_patterns(sections));
        }

        // Early termination: no new journeys found in this iteration
        if journeys.len() == prev_count {
            break;
        }
    }

    journeys.sort_by_key(|j| j.duration);

    // Enrich journeys with Valhalla walking legs
    if !journeys.is_empty() {
        let valhalla_base = format!("http://{}:{}", config.valhalla.host, config.valhalla.port);
        let walking_speed = query.walking_speed;
        let include_maneuvers = query.maneuvers.unwrap_or(false);

        // First/last mile: only when origin or destination are coordinates
        // (addresses). Stop IDs don't need first/last mile because RAPTOR
        // routes directly to/from the stop, and intra-station walking is
        // handled by the transfer graph.
        if from_coord.is_some() || to_coord.is_some() {
            enrich_first_last_mile(
                &mut journeys,
                &raptor_data,
                &valhalla_base,
                from_coord,
                to_coord,
                walking_speed,
                include_maneuvers,
                &effective_date,
            )
            .await;
        }

        // Transfer enrichment: always, regardless of coordinate vs stop_id input
        enrich_transfers(
            &mut journeys,
            &raptor_data,
            &valhalla_base,
            walking_speed,
            include_maneuvers,
        )
        .await;
    }

    tag_journeys(&mut journeys);

    HttpResponse::Ok().json(JourneysResponse { journeys })
}

/// Compute and prepend/append first-mile and last-mile walking sections.
///
/// For each journey, identifies the first and last public transport stops,
/// then calls Valhalla to compute a pedestrian route from the origin
/// coordinate to the first PT stop (first-mile) and from the last PT stop
/// to the destination coordinate (last-mile). Results are cached by stop
/// index to avoid redundant Valhalla calls across journeys.
///
/// Leading/trailing transfer sections are removed when a walking leg
/// replaces them (the user walks directly from origin to the PT stop).
#[allow(clippy::too_many_arguments)]
async fn enrich_first_last_mile(
    journeys: &mut [Journey],
    raptor_data: &RaptorData,
    valhalla_base: &str,
    from_coord: Option<(f64, f64)>,
    to_coord: Option<(f64, f64)>,
    walking_speed: Option<f64>,
    include_maneuvers: bool,
    date: &str,
) {
    let mut first_mile_cache: rustc_hash::FxHashMap<usize, Option<Arc<valhalla::WalkLeg>>> =
        rustc_hash::FxHashMap::default();
    let mut last_mile_cache: rustc_hash::FxHashMap<usize, Option<Arc<valhalla::WalkLeg>>> =
        rustc_hash::FxHashMap::default();

    for journey in journeys.iter_mut() {
        let first_stop_idx = journey
            .sections
            .iter()
            .find(|s| s.section_type == "public_transport")
            .and_then(|s| s.from.stop_point.as_ref())
            .and_then(|sp| raptor_data.stop_index.get(&sp.id))
            .copied();
        let last_stop_idx = journey
            .sections
            .iter()
            .rev()
            .find(|s| s.section_type == "public_transport")
            .and_then(|s| s.to.stop_point.as_ref())
            .and_then(|sp| raptor_data.stop_index.get(&sp.id))
            .copied();

        // Compute first-mile and last-mile in parallel
        let first_mile_fut = async {
            if let (Some(from_c), Some(stop_idx)) = (from_coord, first_stop_idx) {
                let stop = &raptor_data.stops[stop_idx];
                if (from_c.0 - stop.stop_lon).abs() < 1e-6
                    && (from_c.1 - stop.stop_lat).abs() < 1e-6
                {
                    return (stop_idx, None);
                }
                if let Some(cached) = first_mile_cache.get(&stop_idx) {
                    return (stop_idx, cached.clone());
                }
                let result = valhalla::pedestrian_route(
                    valhalla_base,
                    from_c,
                    (stop.stop_lon, stop.stop_lat),
                    walking_speed,
                    false, // first mile: penalize stairs
                )
                .await
                .map(Arc::new);
                (stop_idx, result)
            } else {
                (0, None)
            }
        };

        let last_mile_fut = async {
            if let (Some(to_c), Some(stop_idx)) = (to_coord, last_stop_idx) {
                let stop = &raptor_data.stops[stop_idx];
                if (to_c.0 - stop.stop_lon).abs() < 1e-6 && (to_c.1 - stop.stop_lat).abs() < 1e-6 {
                    return (stop_idx, None);
                }
                if let Some(cached) = last_mile_cache.get(&stop_idx) {
                    return (stop_idx, cached.clone());
                }
                let result = valhalla::pedestrian_route(
                    valhalla_base,
                    (stop.stop_lon, stop.stop_lat),
                    to_c,
                    walking_speed,
                    false, // last mile: penalize stairs
                )
                .await
                .map(Arc::new);
                (stop_idx, result)
            } else {
                (0, None)
            }
        };

        let ((fm_idx, first_mile), (lm_idx, last_mile)) =
            futures::future::join(first_mile_fut, last_mile_fut).await;

        if from_coord.is_some() && first_stop_idx.is_some() {
            first_mile_cache.insert(fm_idx, first_mile.clone());
        }
        if to_coord.is_some() && last_stop_idx.is_some() {
            last_mile_cache.insert(lm_idx, last_mile.clone());
        }

        // Prepend first-mile walking section
        if let (Some(walk), Some(stop_idx)) = (&first_mile, first_stop_idx) {
            while journey
                .sections
                .first()
                .is_some_and(|s| s.section_type == "transfer")
            {
                let removed = journey.sections.remove(0);
                journey.duration = journey.duration.saturating_sub(removed.duration);
            }

            let first_dep = journey
                .sections
                .first()
                .map(|s| s.departure_date_time.clone())
                .unwrap_or_default();
            let walk_dep_secs = journey
                .sections
                .first()
                .and_then(|s| {
                    raptor::parse_datetime(&s.departure_date_time)
                        .map(|(_, t)| t.saturating_sub(walk.duration))
                })
                .unwrap_or(0);
            let walk_dep = raptor::format_datetime(date, walk_dep_secs);
            let (flon, flat) = from_coord.unwrap();
            let stop = &raptor_data.stops[stop_idx];
            let section = Section {
                section_type: "street_network".to_string(),
                from: Place {
                    id: format!("{flon};{flat}"),
                    name: "".to_string(),
                    stop_point: None,
                },
                to: make_place(stop),
                departure_date_time: walk_dep,
                arrival_date_time: first_dep,
                duration: walk.duration,
                display_informations: None,
                stop_date_times: None,
                shape: Some(walk.shape.clone()),
                distance: Some(walk.distance),
                maneuvers: if include_maneuvers {
                    Some(walk.maneuvers.clone())
                } else {
                    None
                },
                transfer_type: None,
            };
            journey.sections.insert(0, section);
            journey.duration += walk.duration;
            journey.departure_date_time = journey
                .sections
                .first()
                .unwrap()
                .departure_date_time
                .clone();
        }

        // Append last-mile walking section
        if let (Some(walk), Some(stop_idx)) = (&last_mile, last_stop_idx) {
            while journey
                .sections
                .last()
                .is_some_and(|s| s.section_type == "transfer")
            {
                let removed = journey.sections.pop().unwrap();
                journey.duration = journey.duration.saturating_sub(removed.duration);
            }

            let last_arr = journey
                .sections
                .last()
                .map(|s| s.arrival_date_time.clone())
                .unwrap_or_default();
            let walk_arr_secs = journey
                .sections
                .last()
                .and_then(|s| {
                    raptor::parse_datetime(&s.arrival_date_time).map(|(_, t)| t + walk.duration)
                })
                .unwrap_or(0);
            let walk_arr = raptor::format_datetime(date, walk_arr_secs);
            let (tlon, tlat) = to_coord.unwrap();
            let stop = &raptor_data.stops[stop_idx];
            let section = Section {
                section_type: "street_network".to_string(),
                from: make_place(stop),
                to: Place {
                    id: format!("{tlon};{tlat}"),
                    name: "".to_string(),
                    stop_point: None,
                },
                departure_date_time: last_arr,
                arrival_date_time: walk_arr,
                duration: walk.duration,
                display_informations: None,
                stop_date_times: None,
                shape: Some(walk.shape.clone()),
                distance: Some(walk.distance),
                maneuvers: if include_maneuvers {
                    Some(walk.maneuvers.clone())
                } else {
                    None
                },
                transfer_type: None,
            };
            journey.sections.push(section);
            journey.duration += walk.duration;
            journey.arrival_date_time = journey.sections.last().unwrap().arrival_date_time.clone();
        }
    }
}

/// Enrich transfer sections with Valhalla pedestrian walking routes.
///
/// For each transfer section in the journey list, determines whether it is
/// outdoor (different parent stations) or indoor (same station). Outdoor
/// transfers always get a Valhalla walking shape and distance. Indoor
/// transfers only keep Valhalla data when it contains indoor-specific
/// maneuver types (elevator=39, stairs=40, escalator=41, building enter/exit=42-43),
/// since outdoor routes would be incorrect for in-station walks.
async fn enrich_transfers(
    journeys: &mut [Journey],
    raptor_data: &RaptorData,
    valhalla_base: &str,
    walking_speed: Option<f64>,
    include_maneuvers: bool,
) {
    type TransferReq = (usize, usize, (f64, f64), (f64, f64), bool);
    let mut transfer_requests: Vec<TransferReq> = Vec::new();

    for (j_idx, journey) in journeys.iter().enumerate() {
        for (s_idx, section) in journey.sections.iter().enumerate() {
            if section.section_type != "transfer" {
                continue;
            }
            let from_c = section
                .from
                .stop_point
                .as_ref()
                .map(|sp| (sp.coord.lon, sp.coord.lat));
            let to_c = section
                .to
                .stop_point
                .as_ref()
                .map(|sp| (sp.coord.lon, sp.coord.lat));
            if let Some((from, to)) = from_c.zip(to_c) {
                let from_parent = raptor_data
                    .stop_index
                    .get(section.from.id.as_str())
                    .map(|&idx| &raptor_data.stops[idx].parent_station);
                let to_parent = raptor_data
                    .stop_index
                    .get(section.to.id.as_str())
                    .map(|&idx| &raptor_data.stops[idx].parent_station);
                let is_outdoor = match (from_parent, to_parent) {
                    (Some(fp), Some(tp)) => fp.is_empty() || tp.is_empty() || fp != tp,
                    _ => true,
                };
                transfer_requests.push((j_idx, s_idx, from, to, is_outdoor));
            }
        }
    }

    // Always call Valhalla for all transfers (outdoor and indoor) to get
    // shape/distance. Indoor transfers will be filtered post-hoc based on
    // whether Valhalla returns indoor-specific maneuver types.
    let requests_to_call: Vec<_> = transfer_requests.iter().collect();

    let futs: Vec<_> = requests_to_call
        .iter()
        .map(|(_, _, from, to, is_outdoor)| {
            let indoor_friendly = !is_outdoor;
            valhalla::pedestrian_route(valhalla_base, *from, *to, walking_speed, indoor_friendly)
        })
        .collect();
    let results = futures::future::join_all(futs).await;

    for (req, walk) in requests_to_call.iter().zip(results) {
        let (j_idx, s_idx, _, _, is_outdoor) = **req;
        let section = &mut journeys[j_idx].sections[s_idx];

        if is_outdoor {
            section.transfer_type = Some("outdoor".to_string());
            if let Some(walk) = walk {
                section.shape = Some(walk.shape);
                section.distance = Some(walk.distance);
                if include_maneuvers {
                    section.maneuvers = Some(walk.maneuvers);
                }
            }
        } else {
            // Indoor transfer: always keep Valhalla shape/distance when available.
            // Mark as "indoor" if Valhalla confirms indoor maneuvers, otherwise
            // still mark indoor but the trace may follow outdoor sidewalks.
            section.transfer_type = Some("indoor".to_string());
            if let Some(walk) = walk {
                section.shape = Some(walk.shape);
                section.distance = Some(walk.distance);
                if include_maneuvers {
                    section.maneuvers = Some(walk.maneuvers);
                }
            }
        }
    }
}

/// Two journeys are considered duplicates when their public-transport
/// sections use the same stops (from/to) and display the same line label.
/// This catches itineraries that differ only in departure time but follow
/// the exact same route.
fn journey_section_key(j: &Journey) -> Vec<(&str, &str, &str)> {
    j.sections
        .iter()
        .filter(|s| s.section_type == "public_transport")
        .map(|s| {
            let label = s
                .display_informations
                .as_ref()
                .map(|d| d.label.as_str())
                .unwrap_or("");
            (s.from.id.as_str(), s.to.id.as_str(), label)
        })
        .collect()
}

fn journey_sections_equal(a: &Journey, b: &Journey) -> bool {
    journey_section_key(a) == journey_section_key(b)
}

use crate::util::parse_coord;

// ---------------------------------------------------------------------------
// Stop resolution & mode exclusion
// ---------------------------------------------------------------------------

/// Resolve a stop reference (ID or "lon;lat" coordinates) to a list of
/// `(stop_idx, walk_duration_secs)` pairs. For coordinates, returns all
/// nearby stops within `max_dist` meters; for a stop ID, returns a single
/// stop with zero walk time.
fn resolve_stops(
    data: &RaptorData,
    input: &str,
    coord: Option<(f64, f64)>,
    max_dist: u32,
    walking_speed: f64,
) -> Vec<(usize, u32)> {
    if let Some((lon, lat)) = coord {
        data.find_nearby_stops(lon, lat, max_dist, walking_speed)
    } else if let Some(idx) = data.resolve_stop(input, max_dist) {
        // Stop ID resolution: no parent_station expansion needed here because
        // RAPTOR's transfer graph already connects sibling stops via intra-station
        // transfers (built in build_transfers). We only need to handle the case
        // where the selected stop is a station node (no patterns) — then resolve
        // to its child stops.
        if !data.stop_patterns[idx].is_empty() {
            // Case 1: the stop itself is served by trips → use it directly
            vec![(idx, 0)]
        } else {
            // Case 2: station node without patterns → find child stops that
            // reference this stop as their parent_station
            let stop_id = &data.stops[idx].stop_id;
            let children: Vec<(usize, u32)> = data
                .stops
                .iter()
                .enumerate()
                .filter(|(child_idx, child)| {
                    child.parent_station == *stop_id && !data.stop_patterns[*child_idx].is_empty()
                })
                .map(|(child_idx, _)| (child_idx, 0))
                .collect();
            if children.is_empty() {
                // Case 3: try as a sibling — find siblings via parent_station
                let parent = &data.stops[idx].parent_station;
                if !parent.is_empty() {
                    data.stops
                        .iter()
                        .enumerate()
                        .filter(|(sib_idx, sib)| {
                            *sib_idx != idx
                                && sib.parent_station == *parent
                                && !data.stop_patterns[*sib_idx].is_empty()
                        })
                        .map(|(sib_idx, _)| (sib_idx, 0))
                        .collect()
                } else {
                    vec![]
                }
            } else {
                children
            }
        }
    } else {
        vec![]
    }
}

/// GTFS route_type code to commercial mode name.
fn route_type_to_mode(route_type: u16) -> &'static str {
    match route_type {
        0 => "tramway",
        1 => "metro",
        2 => "rail",
        3 => "bus",
        7 => "funicular",
        _ => "other",
    }
}

/// Build display info (line label, color, mode) from a GTFS route.
fn make_display_info(route: &crate::gtfs::Route, headsign: &str) -> DisplayInfo {
    DisplayInfo {
        network: String::new(),
        direction: headsign.to_string(),
        commercial_mode: route_type_to_mode(route.route_type).to_string(),
        label: route.route_short_name.clone(),
        color: route.route_color.clone(),
        text_color: route.route_text_color.clone(),
    }
}

/// Compute the set of pattern indices to exclude based on forbidden transport
/// mode names (comma-separated: "metro,bus,rail").
fn compute_mode_exclusions(
    data: &RaptorData,
    forbidden_modes_str: &str,
) -> rustc_hash::FxHashSet<usize> {
    if forbidden_modes_str.is_empty() {
        return rustc_hash::FxHashSet::default();
    }
    let forbidden_types: rustc_hash::FxHashSet<u16> = forbidden_modes_str
        .split(',')
        .filter_map(|m| match m.trim() {
            "tramway" => Some(0),
            "metro" => Some(1),
            "rail" => Some(2),
            "bus" => Some(3),
            "funicular" => Some(7),
            _ => None,
        })
        .collect();
    data.patterns
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            data.routes
                .get(&p.route_id)
                .is_some_and(|r| forbidden_types.contains(&r.route_type))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Check if the current PT section should merge with a previous one.
///
/// GTFS feeds sometimes split a continuous line into multiple trip_ids at
/// intermediate stops. When two consecutive PT sections share the same route
/// label and connect at the same stop, they represent the same physical ride
/// and should be merged into a single section. A transfer section between
/// them is also absorbed.
///
/// Returns `(merge_target_index, should_merge)`.
fn find_merge_target(
    api_sections: &[Section],
    from_stop: &crate::gtfs::Stop,
    display_info: &Option<DisplayInfo>,
) -> (usize, bool) {
    let len = api_sections.len();
    let from_id = &from_stop.stop_id;
    let new_label = display_info.as_ref().map(|d| d.label.as_str());

    // Helper: check if a section can be merged with the new one
    let can_merge = |s: &Section| -> bool {
        s.section_type == "public_transport"
            && s.to.id == *from_id
            && s.display_informations.is_some()
            && new_label.is_some()
            && s.display_informations.as_ref().unwrap().label == new_label.unwrap()
    };

    // Case 1: previous is a transfer, the one before that is a mergeable PT section
    if len >= 2
        && api_sections[len - 1].section_type == "transfer"
        && can_merge(&api_sections[len - 2])
    {
        return (len - 2, true);
    }
    // Case 2: previous section is directly a mergeable PT section
    if len >= 1 && can_merge(&api_sections[len - 1]) {
        return (len - 1, true);
    }
    (0, false)
}

// ---------------------------------------------------------------------------
// Journey building
// ---------------------------------------------------------------------------

/// Convert raw RAPTOR journey sections into the API response format.
///
/// Walks through each section, building display info for PT legs, merging
/// consecutive sections on the same line (trip splits), and counting transfers.
fn build_journey(data: &RaptorData, sections: &[raptor::JourneySection], date: &str) -> Journey {
    let mut api_sections: Vec<Section> = Vec::new();
    let mut nb_transfers: u32 = 0;
    let mut pt_count: u32 = 0;

    for section in sections {
        let from_stop = &data.stops[section.from_stop];
        let to_stop = &data.stops[section.to_stop];
        let dep_dt = raptor::format_datetime(date, section.departure_time);
        let arr_dt = raptor::format_datetime(date, section.arrival_time);
        let duration = section.arrival_time.saturating_sub(section.departure_time);

        match section.section_type {
            SectionType::PublicTransport => {
                pt_count += 1;
                if pt_count > 1 {
                    nb_transfers += 1;
                }

                let mut display_info = None;
                let mut stop_date_times = None;

                if let (Some(pat_idx), Some(trip_idx), Some(bp), Some(ap)) = (
                    section.pattern_idx,
                    section.trip_idx,
                    section.board_pos,
                    section.alight_pos,
                ) {
                    let pattern = &data.patterns[pat_idx];
                    let trip = &pattern.trips[trip_idx];

                    if let Some(route) = data.routes.get(&pattern.route_id) {
                        display_info = Some(make_display_info(route, &trip.headsign));
                    }

                    let mut sdt: Vec<StopDateTime> = Vec::new();
                    for pos in bp..=ap {
                        let st = &data.stops[pattern.stops[pos]];
                        sdt.push(StopDateTime {
                            stop_point: make_stop_point(st),
                            arrival_date_time: raptor::format_datetime(
                                date,
                                trip.stop_times[pos].0,
                            ),
                            departure_date_time: raptor::format_datetime(
                                date,
                                trip.stop_times[pos].1,
                            ),
                        });
                    }
                    stop_date_times = Some(sdt);
                }

                // Merge with previous PT section if same route line and connecting stop.
                // Handles GTFS trip splits where the same physical line changes trip_id.
                let (merge_target_idx, should_merge) =
                    find_merge_target(&api_sections, from_stop, &display_info);

                if should_merge {
                    // Remove intermediate transfer section if present
                    if merge_target_idx < api_sections.len() - 1 {
                        api_sections.pop(); // remove the transfer
                    }
                    let target = &mut api_sections[merge_target_idx];
                    target.to = make_place(to_stop);
                    target.arrival_date_time = arr_dt;
                    target.duration += duration;
                    // Undo the transfer count — this is not a real transfer
                    pt_count -= 1;
                    nb_transfers = nb_transfers.saturating_sub(1);
                    // Append stop_date_times (skip first stop of second segment = last stop of first)
                    if let (Some(existing_sdt), Some(new_sdt)) =
                        (&mut target.stop_date_times, stop_date_times)
                    {
                        for sdt in new_sdt.into_iter().skip(1) {
                            existing_sdt.push(sdt);
                        }
                    }
                } else {
                    api_sections.push(Section {
                        section_type: "public_transport".to_string(),
                        from: make_place(from_stop),
                        to: make_place(to_stop),
                        departure_date_time: dep_dt,
                        arrival_date_time: arr_dt,
                        duration,
                        display_informations: display_info,
                        stop_date_times,
                        shape: None,
                        distance: None,
                        maneuvers: None,
                        transfer_type: None,
                    });
                }
            }
            SectionType::Transfer => {
                api_sections.push(Section {
                    section_type: "transfer".to_string(),
                    from: make_place(from_stop),
                    to: make_place(to_stop),
                    departure_date_time: dep_dt,
                    arrival_date_time: arr_dt,
                    duration,
                    display_informations: None,
                    stop_date_times: None,
                    shape: None,
                    distance: None,
                    maneuvers: None,
                    transfer_type: None,
                });
            }
        }
    }

    let journey_dep = sections.first().map(|s| s.departure_time).unwrap_or(0);
    let journey_arr = sections.last().map(|s| s.arrival_time).unwrap_or(0);

    Journey {
        departure_date_time: raptor::format_datetime(date, journey_dep),
        arrival_date_time: raptor::format_datetime(date, journey_arr),
        duration: journey_arr.saturating_sub(journey_dep),
        nb_transfers,
        tags: Vec::new(),
        sections: api_sections,
    }
}

// ---------------------------------------------------------------------------
// Journey tagging
// ---------------------------------------------------------------------------

/// Compute quality tags for a sorted list of journeys.
///
/// Tags: `fastest`, `least_transfers`, `least_walking`.
/// If all journeys share a tag, only the first keeps it.
fn tag_journeys(journeys: &mut [Journey]) {
    if journeys.is_empty() {
        return;
    }

    let min_duration = journeys.iter().map(|j| j.duration).min().unwrap_or(0);
    let min_transfers = journeys.iter().map(|j| j.nb_transfers).min().unwrap_or(0);

    let walking_times: Vec<u32> = journeys
        .iter()
        .map(|j| {
            j.sections
                .iter()
                .filter(|s| s.section_type == "transfer" || s.section_type == "street_network")
                .map(|s| s.duration)
                .sum()
        })
        .collect();
    let min_walking = walking_times.iter().copied().min().unwrap_or(0);

    for (i, journey) in journeys.iter_mut().enumerate() {
        if journey.duration == min_duration {
            journey.tags.push("fastest".to_string());
        }
        if journey.nb_transfers == min_transfers {
            journey.tags.push("least_transfers".to_string());
        }
        if walking_times[i] == min_walking {
            journey.tags.push("least_walking".to_string());
        }
    }

    for tag in &["fastest", "least_transfers", "least_walking"] {
        let count = journeys
            .iter()
            .filter(|j| j.tags.iter().any(|t| t == tag))
            .count();
        if count == journeys.len() {
            for journey in journeys.iter_mut().skip(1) {
                journey.tags.retain(|t| t != tag);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs;
    use rustc_hash::FxHashMap;

    fn make_test_raptor_data() -> Arc<RaptorData> {
        let mut stops = FxHashMap::default();
        for (id, name, lon, lat) in &[
            ("S1", "StopA", 2.347, 48.858),
            ("S2", "StopB", 2.373, 48.844),
            ("S3", "StopC", 2.395, 48.848),
        ] {
            stops.insert(
                id.to_string(),
                gtfs::Stop {
                    stop_id: id.to_string(),
                    stop_name: name.to_string(),
                    stop_lon: *lon,
                    stop_lat: *lat,
                    parent_station: String::new(),
                },
            );
        }
        let mut routes = FxHashMap::default();
        routes.insert(
            "R1".to_string(),
            gtfs::Route {
                route_id: "R1".to_string(),
                agency_id: "A1".to_string(),
                route_short_name: "1".to_string(),
                route_long_name: "Line 1".to_string(),
                route_type: 1,
                route_color: "FFCD00".to_string(),
                route_text_color: "000000".to_string(),
            },
        );
        let mut trips = FxHashMap::default();
        trips.insert(
            "T1".to_string(),
            gtfs::Trip {
                route_id: "R1".to_string(),
                service_id: "SVC1".to_string(),
                trip_id: "T1".to_string(),
                trip_headsign: "StopC".to_string(),
            },
        );
        let stop_times = vec![
            gtfs::StopTime {
                trip_id: "T1".into(),
                arrival_time: "08:00:00".into(),
                departure_time: "08:01:00".into(),
                stop_id: "S1".into(),
                stop_sequence: 0,
            },
            gtfs::StopTime {
                trip_id: "T1".into(),
                arrival_time: "08:10:00".into(),
                departure_time: "08:11:00".into(),
                stop_id: "S2".into(),
                stop_sequence: 1,
            },
            gtfs::StopTime {
                trip_id: "T1".into(),
                arrival_time: "08:20:00".into(),
                departure_time: "08:21:00".into(),
                stop_id: "S3".into(),
                stop_sequence: 2,
            },
        ];
        let mut calendars = FxHashMap::default();
        calendars.insert(
            "SVC1".to_string(),
            gtfs::Calendar {
                service_id: "SVC1".into(),
                monday: 1,
                tuesday: 1,
                wednesday: 1,
                thursday: 1,
                friday: 1,
                saturday: 1,
                sunday: 1,
                start_date: "20260101".into(),
                end_date: "20261231".into(),
            },
        );
        let gtfs_data = gtfs::GtfsData {
            agencies: vec![],
            routes,
            stops,
            trips,
            stop_times,
            calendars,
            calendar_dates: vec![],
            transfers: vec![],
            pathways: vec![],
        };
        Arc::new(raptor::RaptorData::build(gtfs_data, 120))
    }

    fn make_test_config() -> AppConfig {
        AppConfig::default()
    }

    // -----------------------------------------------------------------------
    // tag_journeys
    // -----------------------------------------------------------------------

    #[test]
    fn tag_journeys_empty() {
        let mut journeys: Vec<Journey> = vec![];
        tag_journeys(&mut journeys);
        assert!(journeys.is_empty());
    }

    #[test]
    fn tag_journeys_single() {
        let mut journeys = vec![Journey {
            departure_date_time: "20260406T080100".into(),
            arrival_date_time: "20260406T082000".into(),
            duration: 1140,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![],
        }];
        tag_journeys(&mut journeys);
        assert!(journeys[0].tags.contains(&"fastest".to_string()));
    }

    #[test]
    fn tag_journeys_diverse() {
        let mut journeys = vec![
            Journey {
                departure_date_time: "20260406T080100".into(),
                arrival_date_time: "20260406T082000".into(),
                duration: 1140,
                nb_transfers: 2,
                tags: vec![],
                sections: vec![Section {
                    section_type: "transfer".into(),
                    from: crate::api::Place {
                        id: "a".into(),
                        name: "a".into(),
                        stop_point: None,
                    },
                    to: crate::api::Place {
                        id: "b".into(),
                        name: "b".into(),
                        stop_point: None,
                    },
                    departure_date_time: "20260406T081000".into(),
                    arrival_date_time: "20260406T081200".into(),
                    duration: 300,
                    display_informations: None,
                    stop_date_times: None,
                    shape: None,
                    distance: None,
                    maneuvers: None,
                    transfer_type: None,
                }],
            },
            Journey {
                departure_date_time: "20260406T080100".into(),
                arrival_date_time: "20260406T083000".into(),
                duration: 1740,
                nb_transfers: 0,
                tags: vec![],
                sections: vec![],
            },
        ];
        tag_journeys(&mut journeys);
        assert!(journeys[0].tags.contains(&"fastest".to_string()));
        assert!(journeys[1].tags.contains(&"least_transfers".to_string()));
        assert!(journeys[1].tags.contains(&"least_walking".to_string()));
    }

    // -----------------------------------------------------------------------
    // build_journey
    // -----------------------------------------------------------------------

    #[test]
    fn build_journey_from_sections() {
        let data = make_test_raptor_data();
        let source = data.stop_index["S1"];
        let target = data.stop_index["S3"];
        let active = data.active_services("20260406");
        let result = raptor::raptor_query(
            &data,
            &[(source, 0)],
            28000,
            &active,
            3,
            &rustc_hash::FxHashSet::default(),
        );
        let section_sets = raptor::reconstruct_journeys(&data, &result, &[(target, 0)]);
        assert!(!section_sets.is_empty());
        let journey = build_journey(&data, &section_sets[0], "20260406");
        assert!(journey.duration > 0);
        assert!(!journey.sections.is_empty());
        assert!(journey.departure_date_time.contains('T'));
        assert!(journey.arrival_date_time.contains('T'));
    }

    #[test]
    fn build_journey_has_display_info() {
        let data = make_test_raptor_data();
        let source = data.stop_index["S1"];
        let target = data.stop_index["S3"];
        let active = data.active_services("20260406");
        let result = raptor::raptor_query(
            &data,
            &[(source, 0)],
            28000,
            &active,
            3,
            &rustc_hash::FxHashSet::default(),
        );
        let section_sets = raptor::reconstruct_journeys(&data, &result, &[(target, 0)]);
        let journey = build_journey(&data, &section_sets[0], "20260406");
        let pt_sections: Vec<_> = journey
            .sections
            .iter()
            .filter(|s| s.section_type == "public_transport")
            .collect();
        assert!(!pt_sections.is_empty());
        let di = pt_sections[0].display_informations.as_ref().unwrap();
        assert_eq!(di.label, "1");
        assert_eq!(di.commercial_mode, "metro");
        assert_eq!(di.color, "FFCD00");
    }

    // -----------------------------------------------------------------------
    // HTTP handler integration tests
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn get_journeys_missing_from() {
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let config = web::Data::new(make_test_config());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(config)
                .service(get_journeys),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?to=S1")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_journeys_missing_to() {
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let config = web::Data::new(make_test_config());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(config)
                .service(get_journeys),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=S1")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_journeys_unknown_stop() {
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let config = web::Data::new(make_test_config());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(config)
                .service(get_journeys),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=UNKNOWN&to=S1")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_journeys_invalid_datetime() {
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let config = web::Data::new(make_test_config());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(config)
                .service(get_journeys),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=S1&to=S3&datetime=bad")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_journeys_success() {
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let config = web::Data::new(make_test_config());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(config)
                .service(get_journeys),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=S1&to=S3&datetime=20260406T080000")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        let journeys = body["journeys"].as_array().unwrap();
        assert!(!journeys.is_empty());
        assert!(journeys[0]["duration"].as_u64().unwrap() > 0);
    }

    #[actix_web::test]
    async fn get_journeys_default_datetime() {
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let config = web::Data::new(make_test_config());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(config)
                .service(get_journeys),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=S1&to=S3")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }
}
