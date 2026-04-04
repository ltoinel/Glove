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

use crate::api::{Section, StopDateTime, make_place, make_stop_point};

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

    let source = match raptor_data.resolve_stop(from_str) {
        Some(idx) => idx,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": { "id": "unknown_object", "message": format!("Stop not found: {from_str}") }
            }));
        }
    };

    let target = match raptor_data.resolve_stop(to_str) {
        Some(idx) => idx,
        None => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": { "id": "unknown_object", "message": format!("Stop not found: {to_str}") }
            }));
        }
    };

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
        .unwrap_or(config.max_transfers);

    let max_duration = query
        .max_duration
        .map(|n| n.max(0) as u32)
        .unwrap_or(config.max_duration);

    let requested = query
        .count
        .or(query.max_nb_journeys)
        .map(|n| (n.max(1) as usize).min(config.max_journeys))
        .unwrap_or(config.max_journeys);

    // Iterative search with pattern exclusion for route diversity
    let mut journeys: Vec<Journey> = Vec::new();
    let mut excluded_patterns = std::collections::HashSet::new();

    for _ in 0..requested {
        let result = raptor::raptor_query(
            &raptor_data,
            source,
            target,
            departure_time,
            &date,
            max_transfers,
            &excluded_patterns,
        );

        let section_sets = raptor::reconstruct_journeys(&raptor_data, &result, target);
        if section_sets.is_empty() {
            break;
        }

        let sections = &section_sets[0];
        let journey = build_journey(&raptor_data, sections, &date);

        if journey.duration > max_duration {
            for sections in &section_sets {
                excluded_patterns.extend(raptor::used_patterns(sections));
            }
            continue;
        }

        let dominated = journeys.iter().any(|existing| {
            existing.departure_date_time == journey.departure_date_time
                && existing.arrival_date_time == journey.arrival_date_time
        });

        if !dominated {
            journeys.push(journey);
        }

        for sections in &section_sets {
            excluded_patterns.extend(raptor::used_patterns(sections));
        }
    }

    journeys.sort_by_key(|j| j.duration);
    tag_journeys(&mut journeys);

    HttpResponse::Ok().json(JourneysResponse { journeys })
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

                api_sections.push(Section {
                    section_type: "public_transport".to_string(),
                    from: make_place(from_stop),
                    to: make_place(to_stop),
                    departure_date_time: dep_dt,
                    arrival_date_time: arr_dt,
                    duration,
                    display_informations: display_info,
                    stop_date_times,
                });
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
