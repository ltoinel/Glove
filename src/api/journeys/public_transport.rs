//! Journey planning endpoint (Navitia-compatible).
//!
//! Runs RAPTOR iteratively with pattern exclusion to produce diverse
//! route alternatives, sorted by duration and tagged with quality labels.

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwap;
use chrono::Timelike;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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

    // Parse origin/destination — may be stop IDs or "lon;lat" coordinates
    let from_coord = parse_coord(from_str);
    let to_coord = parse_coord(to_str);

    let max_dist = config.routing.max_nearest_stop_distance;
    let walking_speed = query.walking_speed.unwrap_or(5.0);

    // Resolve sources: coordinates → all nearby stops; stop ID → single stop
    let sources: Vec<(usize, u32)> = if let Some((lon, lat)) = from_coord {
        raptor_data.find_nearby_stops(lon, lat, max_dist, walking_speed)
    } else if let Some(idx) = raptor_data.resolve_stop(from_str, max_dist) {
        vec![(idx, 0)]
    } else {
        vec![]
    };
    if sources.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": { "id": "unknown_object", "message": format!("No stop found within {} m of {from_str}", max_dist) }
        }));
    }

    // Resolve targets: coordinates → all nearby stops; stop ID → single stop
    let targets: Vec<(usize, u32)> = if let Some((lon, lat)) = to_coord {
        raptor_data.find_nearby_stops(lon, lat, max_dist, walking_speed)
    } else if let Some(idx) = raptor_data.resolve_stop(to_str, max_dist) {
        vec![(idx, 0)]
    } else {
        vec![]
    };
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

    // Pre-compute active services once for all diversity iterations
    let active = raptor_data.active_services(&date);

    // Pre-compute patterns to exclude based on forbidden transport modes
    let forbidden_modes_str = query.forbidden_modes.as_deref().unwrap_or("");
    let mode_excluded: rustc_hash::FxHashSet<usize> = if !forbidden_modes_str.is_empty() {
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
        raptor_data
            .patterns
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                raptor_data
                    .routes
                    .get(&p.route_id)
                    .is_some_and(|r| forbidden_types.contains(&r.route_type))
            })
            .map(|(i, _)| i)
            .collect()
    } else {
        rustc_hash::FxHashSet::default()
    };

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
            departure_time,
            &active,
            max_transfers,
            &excluded_patterns,
        );

        let section_sets = raptor::reconstruct_journeys(&raptor_data, &result, &targets);
        if section_sets.is_empty() {
            break;
        }

        // Exploit all Pareto-optimal journeys from this RAPTOR run
        for sections in &section_sets {
            let journey = build_journey(&raptor_data, sections, &date);

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
    }

    journeys.sort_by_key(|j| j.duration);

    // Compute first/last mile walking legs per journey.
    // Each journey may start/end at a different stop, so we compute the walking
    // leg to/from the actual first/last PT stop (not the nearest-stop proxy).
    // Results are cached by stop index to avoid redundant Valhalla calls.
    if !journeys.is_empty() && (from_coord.is_some() || to_coord.is_some()) {
        let valhalla_base = format!("http://{}:{}", config.valhalla.host, config.valhalla.port);
        let walking_speed = query.walking_speed;

        // Cache: stop_idx → WalkLeg (avoid recomputing for the same stop across journeys)
        let mut first_mile_cache: rustc_hash::FxHashMap<usize, Option<valhalla::WalkLeg>> =
            rustc_hash::FxHashMap::default();
        let mut last_mile_cache: rustc_hash::FxHashMap<usize, Option<valhalla::WalkLeg>> =
            rustc_hash::FxHashMap::default();

        for journey in &mut journeys {
            // Identify the actual first and last PT stops of this journey
            let first_pt_section = journey
                .sections
                .iter()
                .find(|s| s.section_type == "public_transport");
            let last_pt_section = journey
                .sections
                .iter()
                .rev()
                .find(|s| s.section_type == "public_transport");

            let first_stop_idx = first_pt_section
                .and_then(|s| s.from.stop_point.as_ref())
                .and_then(|sp| raptor_data.stop_index.get(&sp.id))
                .copied();
            let last_stop_idx = last_pt_section
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
                        &valhalla_base,
                        from_c,
                        (stop.stop_lon, stop.stop_lat),
                        walking_speed,
                    )
                    .await;
                    (stop_idx, result)
                } else {
                    (0, None)
                }
            };

            let last_mile_fut = async {
                if let (Some(to_c), Some(stop_idx)) = (to_coord, last_stop_idx) {
                    let stop = &raptor_data.stops[stop_idx];
                    if (to_c.0 - stop.stop_lon).abs() < 1e-6
                        && (to_c.1 - stop.stop_lat).abs() < 1e-6
                    {
                        return (stop_idx, None);
                    }
                    if let Some(cached) = last_mile_cache.get(&stop_idx) {
                        return (stop_idx, cached.clone());
                    }
                    let result = valhalla::pedestrian_route(
                        &valhalla_base,
                        (stop.stop_lon, stop.stop_lat),
                        to_c,
                        walking_speed,
                    )
                    .await;
                    (stop_idx, result)
                } else {
                    (0, None)
                }
            };

            let ((fm_idx, first_mile), (lm_idx, last_mile)) =
                futures::future::join(first_mile_fut, last_mile_fut).await;

            // Cache results
            if from_coord.is_some() && first_stop_idx.is_some() {
                first_mile_cache.insert(fm_idx, first_mile.clone());
            }
            if to_coord.is_some() && last_stop_idx.is_some() {
                last_mile_cache.insert(lm_idx, last_mile.clone());
            }

            // Prepend first-mile walking section
            if let (Some(walk), Some(stop_idx)) = (&first_mile, first_stop_idx) {
                // Remove leading transfer sections that are now redundant
                // (the user walks directly from origin to the first PT stop)
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
                let walk_dep = raptor::format_datetime(&date, walk_dep_secs);
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
                    maneuvers: Some(walk.maneuvers.clone()),
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
                // Remove trailing transfer sections that are now redundant
                // (the user walks directly from the last PT stop to destination)
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
                let walk_arr = raptor::format_datetime(&date, walk_arr_secs);
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
                    maneuvers: Some(walk.maneuvers.clone()),
                };
                journey.sections.push(section);
                journey.duration += walk.duration;
                journey.arrival_date_time =
                    journey.sections.last().unwrap().arrival_date_time.clone();
            }
        }
    }

    tag_journeys(&mut journeys);

    HttpResponse::Ok().json(JourneysResponse { journeys })
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

/// Parse a `"lon;lat"` string into `(lon, lat)`.
fn parse_coord(s: &str) -> Option<(f64, f64)> {
    let (lon_str, lat_str) = s.split_once(';')?;
    Some((lon_str.parse().ok()?, lat_str.parse().ok()?))
}

// ---------------------------------------------------------------------------
// Journey building
// ---------------------------------------------------------------------------

/// Convert raw RAPTOR journey sections into the API response format.
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
                        let mode = match route.route_type {
                            0 => "tramway",
                            1 => "metro",
                            2 => "rail",
                            3 => "bus",
                            7 => "funicular",
                            _ => "other",
                        };
                        display_info = Some(DisplayInfo {
                            network: String::new(),
                            direction: trip.headsign.clone(),
                            commercial_mode: mode.to_string(),
                            label: route.route_short_name.clone(),
                            color: route.route_color.clone(),
                            text_color: route.route_text_color.clone(),
                        });
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

                // Merge with previous PT section if same route and connecting stop
                // (handles GTFS trip splits where the same physical line changes trip_id)
                // The previous section may be a transfer at the connecting stop — look past it.
                let (merge_target_idx, should_merge) = {
                    let len = api_sections.len();
                    if len >= 2
                        && api_sections[len - 1].section_type == "transfer"
                        && api_sections[len - 2].section_type == "public_transport"
                        && api_sections[len - 2].to.id == make_place(from_stop).id
                        && api_sections[len - 2].display_informations.is_some()
                        && display_info.is_some()
                        && api_sections[len - 2]
                            .display_informations
                            .as_ref()
                            .unwrap()
                            .label
                            == display_info.as_ref().unwrap().label
                    {
                        (len - 2, true)
                    } else if len >= 1
                        && api_sections[len - 1].section_type == "public_transport"
                        && api_sections[len - 1].to.id == make_place(from_stop).id
                        && api_sections[len - 1].display_informations.is_some()
                        && display_info.is_some()
                        && api_sections[len - 1]
                            .display_informations
                            .as_ref()
                            .unwrap()
                            .label
                            == display_info.as_ref().unwrap().label
                    {
                        (len - 1, true)
                    } else {
                        (0, false)
                    }
                };

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
                .filter(|s| s.section_type == "transfer")
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
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_test_raptor_data() -> Arc<RaptorData> {
        let mut stops = HashMap::new();
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
        let mut routes = HashMap::new();
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
        let mut trips = HashMap::new();
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
        let mut calendars = HashMap::new();
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
