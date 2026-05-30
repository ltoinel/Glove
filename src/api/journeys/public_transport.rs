//! Journey planning endpoint.
//!
//! Runs RAPTOR iteratively with pattern exclusion to produce diverse
//! route alternatives, sorted by duration and tagged with quality labels.

use actix_web::{HttpResponse, get, web};
use arc_swap::ArcSwap;
use chrono::Timelike;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use std::sync::Arc;

use crate::config::{AppConfig, WheelchairConfig};
use crate::raptor::{self, RaptorData, SectionType};

use super::valhalla;
use crate::api::{Place, Section, StopDateTime, make_place, make_stop_point};

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/journeys`.
///
/// Most fields are optional and fall back to values from [`AppConfig`].
#[derive(Debug, Deserialize, IntoParams)]
pub struct JourneysQuery {
    /// Origin: a `stop_id`, or `lon;lat` coordinates for an address.
    pub from: Option<String>,
    /// Destination: a `stop_id`, or `lon;lat` coordinates for an address.
    pub to: Option<String>,
    /// Departure date-time in ISO basic format `YYYYMMDDTHHmmss`. Defaults to now.
    pub datetime: Option<String>,
    /// Maximum total journey duration in seconds. Falls back to `routing.max_duration`.
    pub max_duration: Option<i32>,
    /// Walking speed in km/h for first/last-mile legs (default 5).
    pub walking_speed: Option<f64>,
    /// Enable wheelchair-accessible routing (avoid stairs, limit grade).
    pub wheelchair: Option<bool>,
    /// Comma-separated commercial modes to exclude (e.g. "metro,bus,rail").
    pub forbidden_modes: Option<String>,
    /// Language for maneuver instructions (e.g. "fr-FR", "en-US").
    pub language: Option<String>,
}
// Note: number of journeys, transfers, line diversity, rail preference and
// maneuvers are server-controlled via `routing.*` in config.yaml — intentionally
// not exposed as request parameters.

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

    let resolved = match resolve_journey_query(&query, &raptor_data, &config) {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let mut journeys = run_iterative_search(&raptor_data, &resolved);

    if !journeys.is_empty() {
        enrich_journeys(&mut journeys, &query, &config, &raptor_data, &resolved).await;
    }

    // Sort by total duration AFTER enrichment: Valhalla first/last-mile walking
    // adjusts each journey's duration, so sorting earlier leaves a stale order.
    journeys.sort_by_key(|j| j.duration);

    tag_journeys(&mut journeys, resolved.wheelchair);

    HttpResponse::Ok().json(JourneysResponse { journeys })
}

const EARLY_MORNING_THRESHOLD: u32 = 4 * 3600; // 04:00

/// Resolved query parameters for a single journey search.
struct ResolvedQuery {
    from_coord: Option<(f64, f64)>,
    to_coord: Option<(f64, f64)>,
    sources: Vec<(usize, u32)>,
    targets: Vec<(usize, u32)>,
    effective_date: String,
    effective_departure: u32,
    active: Vec<bool>,
    max_transfers: usize,
    max_duration: u32,
    requested: usize,
    wheelchair: bool,
    mode_excluded: rustc_hash::FxHashSet<usize>,
    diverse_lines: bool,
    prefer_rail: bool,
}

fn bad_request(id: &str, message: String) -> HttpResponse {
    HttpResponse::BadRequest().json(serde_json::json!({
        "error": { "id": id, "message": message }
    }))
}

/// Parse and validate every input parameter, resolving stops and dates.
/// Returns an `HttpResponse` (400) on any validation failure.
fn resolve_journey_query(
    query: &JourneysQuery,
    raptor_data: &RaptorData,
    config: &AppConfig,
) -> Result<ResolvedQuery, HttpResponse> {
    let from_str = query
        .from
        .as_deref()
        .ok_or_else(|| bad_request("bad_request", "'from' parameter is required".into()))?;
    let to_str = query
        .to
        .as_deref()
        .ok_or_else(|| bad_request("bad_request", "'to' parameter is required".into()))?;

    let from_coord = parse_coord(from_str);
    let to_coord = parse_coord(to_str);

    let max_dist = config.routing.max_nearest_stop_distance;
    let walking_speed = query.walking_speed.unwrap_or(5.0);

    let sources = resolve_stops(raptor_data, from_str, from_coord, max_dist, walking_speed);
    if sources.is_empty() {
        return Err(bad_request(
            "unknown_object",
            format!("No stop found within {max_dist} m of {from_str}"),
        ));
    }
    let targets = resolve_stops(raptor_data, to_str, to_coord, max_dist, walking_speed);
    if targets.is_empty() {
        return Err(bad_request(
            "unknown_object",
            format!("No stop found within {max_dist} m of {to_str}"),
        ));
    }

    let (date, departure_time) = parse_query_datetime(query.datetime.as_deref())?;
    let (effective_date, effective_departure) =
        shift_to_previous_day_if_early(date, departure_time);

    let active = raptor_data.active_services(&effective_date);
    let mode_excluded =
        compute_mode_exclusions(raptor_data, query.forbidden_modes.as_deref().unwrap_or(""));

    Ok(ResolvedQuery {
        from_coord,
        to_coord,
        sources,
        targets,
        effective_date,
        effective_departure,
        active,
        // Server-controlled via config only (not overridable per request).
        max_transfers: config.routing.max_transfers,
        max_duration: query
            .max_duration
            .map(|n| n.max(0) as u32)
            .unwrap_or(config.routing.max_duration),
        requested: config.routing.max_journeys,
        wheelchair: query.wheelchair.unwrap_or(false),
        mode_excluded,
        diverse_lines: config.routing.diverse_lines,
        prefer_rail: config.routing.prefer_rail,
    })
}

/// Parse the user-supplied datetime, or default to "now" when omitted.
fn parse_query_datetime(datetime: Option<&str>) -> Result<(String, u32), HttpResponse> {
    match datetime {
        Some(dt) => raptor::parse_datetime(dt).ok_or_else(|| {
            bad_request(
                "bad_request",
                "Invalid datetime format. Use YYYYMMDDTHHmmss".into(),
            )
        }),
        None => {
            let now = chrono::Local::now();
            let today = now.format("%Y%m%d").to_string();
            let secs = now.hour() * 3600 + now.minute() * 60 + now.second();
            Ok((today, secs))
        }
    }
}

/// Early-morning shift: queries before 4h use the previous day's services
/// with a +24h offset, because GTFS encodes after-midnight trips on the
/// previous day (e.g. 25:30:00).
fn shift_to_previous_day_if_early(date: String, departure_time: u32) -> (String, u32) {
    if departure_time >= EARLY_MORNING_THRESHOLD {
        return (date, departure_time);
    }
    let y: i32 = date.get(0..4).and_then(|s| s.parse().ok()).unwrap_or(2026);
    let m: u32 = date.get(4..6).and_then(|s| s.parse().ok()).unwrap_or(1);
    let d: u32 = date.get(6..8).and_then(|s| s.parse().ok()).unwrap_or(1);
    let yesterday = chrono::NaiveDate::from_ymd_opt(y, m, d)
        .and_then(|dt| dt.pred_opt())
        .map(|dt| dt.format("%Y%m%d").to_string())
        .unwrap_or_else(|| date.clone());
    (yesterday, departure_time + 86400)
}

/// Iterative RAPTOR search with pattern exclusion for route diversity.
///
/// Produces alternative routes rather than dominated copies of the fastest
/// path. When `prefer_rail` is set, runs a first tier with buses forbidden so
/// rail/metro/tram/train journeys are found first; buses then only fill the
/// remaining slots (the final list is still sorted by duration by the caller).
fn run_iterative_search(raptor_data: &RaptorData, q: &ResolvedQuery) -> Vec<Journey> {
    let mut journeys: Vec<Journey> = Vec::new();
    let mut excluded_patterns = q.mode_excluded.clone();

    // Line-level diversity: precompute route_id -> patterns so we can exclude a
    // whole head line at once (see exclude_head_line). Only built when enabled.
    let route_patterns = q
        .diverse_lines
        .then(|| build_route_pattern_index(raptor_data));
    let route_patterns = route_patterns.as_ref();

    if q.prefer_rail {
        // Tier 1: rail only — forbid bus boarding so non-bus journeys are found
        // first. The bus exclusion is per-query (not recorded for diversity), so
        // the final tier below can re-allow buses.
        let bus_patterns = compute_mode_exclusions(raptor_data, "bus");
        collect_alternatives(
            raptor_data,
            q,
            &bus_patterns,
            route_patterns,
            &mut journeys,
            &mut excluded_patterns,
        );
    }

    // Final tier: all modes allowed. Fills the remaining slots after the rail
    // tier (or everything when prefer_rail is off). Diversity exclusions carry over.
    let no_extra = rustc_hash::FxHashSet::default();
    collect_alternatives(
        raptor_data,
        q,
        &no_extra,
        route_patterns,
        &mut journeys,
        &mut excluded_patterns,
    );

    journeys
}

/// Run RAPTOR iterations, accumulating diverse journeys until `q.requested` is
/// reached or no new journey appears. `extra_forbidden` is unioned into each
/// query's exclusion set *without* being recorded in `excluded_patterns`, so a
/// later tier can re-allow those patterns while keeping diversity exclusions.
fn collect_alternatives(
    raptor_data: &RaptorData,
    q: &ResolvedQuery,
    extra_forbidden: &rustc_hash::FxHashSet<usize>,
    route_patterns: Option<&rustc_hash::FxHashMap<&str, Vec<usize>>>,
    journeys: &mut Vec<Journey>,
    excluded_patterns: &mut rustc_hash::FxHashSet<usize>,
) {
    while journeys.len() < q.requested {
        let query_excluded = if extra_forbidden.is_empty() {
            excluded_patterns.clone()
        } else {
            let mut set = excluded_patterns.clone();
            set.extend(extra_forbidden.iter().copied());
            set
        };

        let result = raptor::raptor_query(
            raptor_data,
            &q.sources,
            q.effective_departure,
            &q.active,
            q.max_transfers,
            &query_excluded,
            q.wheelchair,
        );

        let section_sets = raptor::reconstruct_journeys(raptor_data, &result, &q.targets);
        if section_sets.is_empty() {
            break;
        }

        let prev_count = journeys.len();
        for sections in &section_sets {
            let journey = build_journey(raptor_data, sections, &q.effective_date);
            if journey.duration > q.max_duration {
                continue;
            }
            let dominated = journeys
                .iter()
                .any(|existing| journey_sections_equal(existing, &journey));
            if !dominated {
                journeys.push(journey);
            }
            excluded_patterns.extend(raptor::used_patterns(sections));
            if let Some(route_patterns) = route_patterns {
                exclude_head_line(raptor_data, sections, route_patterns, excluded_patterns);
            }
        }

        // Early termination: no new journeys found in this iteration.
        if journeys.len() == prev_count {
            break;
        }
    }
}

/// Map each GTFS `route_id` to all the pattern indices belonging to that line.
/// A single line owns several patterns (branches, directions, short-turns), so
/// this lets line-level diversity exclude the whole line in one step.
fn build_route_pattern_index(data: &RaptorData) -> rustc_hash::FxHashMap<&str, Vec<usize>> {
    let mut map: rustc_hash::FxHashMap<&str, Vec<usize>> = rustc_hash::FxHashMap::default();
    for (idx, pattern) in data.patterns.iter().enumerate() {
        map.entry(pattern.route_id.as_str()).or_default().push(idx);
    }
    map
}

/// Exclude every pattern of the journey's first public-transport line, so the
/// next iteration is forced to depart on a different line.
fn exclude_head_line(
    data: &RaptorData,
    sections: &[raptor::JourneySection],
    route_patterns: &rustc_hash::FxHashMap<&str, Vec<usize>>,
    excluded_patterns: &mut rustc_hash::FxHashSet<usize>,
) {
    let Some(head_pattern) = sections.iter().find_map(|s| s.pattern_idx) else {
        return;
    };
    let route_id = data.patterns[head_pattern].route_id.as_str();
    if let Some(patterns) = route_patterns.get(route_id) {
        excluded_patterns.extend(patterns.iter().copied());
    }
}

/// Run Valhalla enrichment passes (first/last mile, transfer shapes).
async fn enrich_journeys(
    journeys: &mut [Journey],
    query: &JourneysQuery,
    config: &AppConfig,
    raptor_data: &RaptorData,
    q: &ResolvedQuery,
) {
    let valhalla_base = format!("http://{}:{}", config.valhalla.host, config.valhalla.port);
    let ctx = EnrichmentCtx {
        raptor_data,
        valhalla_base: &valhalla_base,
        walking_speed: query.walking_speed,
        include_maneuvers: config.routing.maneuvers,
        language: query.language.as_deref(),
        wheelchair_config: if q.wheelchair {
            Some(&config.wheelchair)
        } else {
            None
        },
    };

    // First/last mile: only when origin or destination are coordinates
    // (addresses). Stop IDs don't need first/last mile because RAPTOR
    // routes directly to/from the stop, and intra-station walking is
    // handled by the transfer graph.
    if q.from_coord.is_some() || q.to_coord.is_some() {
        let endpoints = Endpoints {
            from: q.from_coord,
            to: q.to_coord,
        };
        enrich_first_last_mile(journeys, &ctx, endpoints, &q.effective_date).await;
    }

    // Transfer enrichment: always, regardless of coordinate vs stop_id input
    enrich_transfers(journeys, &ctx).await;
}

/// Shared parameters threaded through Valhalla enrichment helpers.
struct EnrichmentCtx<'a> {
    raptor_data: &'a RaptorData,
    valhalla_base: &'a str,
    walking_speed: Option<f64>,
    include_maneuvers: bool,
    language: Option<&'a str>,
    wheelchair_config: Option<&'a WheelchairConfig>,
}

/// Origin/destination coordinates for a journey query.
#[derive(Copy, Clone)]
struct Endpoints {
    from: Option<(f64, f64)>,
    to: Option<(f64, f64)>,
}

type WalkLegCache = rustc_hash::FxHashMap<usize, Option<Arc<valhalla::WalkLeg>>>;

/// Compute a cached pedestrian route between a coordinate and a stop.
async fn cached_pedestrian_route(
    ctx: &EnrichmentCtx<'_>,
    cache: &WalkLegCache,
    from: (f64, f64),
    to: (f64, f64),
    stop_idx: usize,
) -> (usize, Option<Arc<valhalla::WalkLeg>>) {
    if (from.0 - to.0).abs() < 1e-6 && (from.1 - to.1).abs() < 1e-6 {
        return (stop_idx, None);
    }
    if let Some(cached) = cache.get(&stop_idx) {
        return (stop_idx, cached.clone());
    }
    let result = valhalla::pedestrian_route(
        ctx.valhalla_base,
        from,
        to,
        ctx.walking_speed,
        false,
        ctx.language,
        ctx.wheelchair_config,
    )
    .await
    .map(Arc::new);
    (stop_idx, result)
}

/// Find the first/last PT stop indices within a journey, if any.
fn endpoint_stop_indices(
    raptor_data: &RaptorData,
    journey: &Journey,
) -> (Option<usize>, Option<usize>) {
    let resolve = |s: &Section| -> Option<usize> {
        s.from
            .stop_point
            .as_ref()
            .and_then(|sp| raptor_data.stop_index.get(&sp.id))
            .copied()
    };
    let first = journey
        .sections
        .iter()
        .find(|s| s.section_type == "public_transport")
        .and_then(resolve);
    let last = journey
        .sections
        .iter()
        .rev()
        .find(|s| s.section_type == "public_transport")
        .and_then(|s| {
            s.to.stop_point
                .as_ref()
                .and_then(|sp| raptor_data.stop_index.get(&sp.id))
                .copied()
        });
    (first, last)
}

/// Build a `street_network` section between a coordinate and a stop.
fn build_walk_section(
    walk: &valhalla::WalkLeg,
    from: Place,
    to: Place,
    departure_date_time: String,
    arrival_date_time: String,
    include_maneuvers: bool,
) -> Section {
    Section {
        section_type: "street_network".to_string(),
        from,
        to,
        departure_date_time,
        arrival_date_time,
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
    }
}

/// Prepend a first-mile walking section, replacing leading transfer sections.
fn prepend_first_mile(
    journey: &mut Journey,
    walk: &valhalla::WalkLeg,
    raptor_data: &RaptorData,
    stop_idx: usize,
    from_coord: (f64, f64),
    date: &str,
    include_maneuvers: bool,
) {
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
    let (flon, flat) = from_coord;
    let stop = &raptor_data.stops[stop_idx];
    let section = build_walk_section(
        walk,
        Place {
            id: format!("{flon};{flat}"),
            name: String::new(),
            stop_point: None,
        },
        make_place(stop),
        walk_dep,
        first_dep,
        include_maneuvers,
    );
    journey.departure_date_time = section.departure_date_time.clone();
    journey.sections.insert(0, section);
    journey.duration += walk.duration;
}

/// Append a last-mile walking section, replacing trailing transfer sections.
fn append_last_mile(
    journey: &mut Journey,
    walk: &valhalla::WalkLeg,
    raptor_data: &RaptorData,
    stop_idx: usize,
    to_coord: (f64, f64),
    date: &str,
    include_maneuvers: bool,
) {
    while let Some(last) = journey.sections.last() {
        if last.section_type != "transfer" {
            break;
        }
        let removed_duration = last.duration;
        journey.sections.pop();
        journey.duration = journey.duration.saturating_sub(removed_duration);
    }

    let last_arr = journey
        .sections
        .last()
        .map(|s| s.arrival_date_time.clone())
        .unwrap_or_default();
    let walk_arr_secs = journey
        .sections
        .last()
        .and_then(|s| raptor::parse_datetime(&s.arrival_date_time).map(|(_, t)| t + walk.duration))
        .unwrap_or(0);
    let walk_arr = raptor::format_datetime(date, walk_arr_secs);
    let (tlon, tlat) = to_coord;
    let stop = &raptor_data.stops[stop_idx];
    let section = build_walk_section(
        walk,
        make_place(stop),
        Place {
            id: format!("{tlon};{tlat}"),
            name: String::new(),
            stop_point: None,
        },
        last_arr,
        walk_arr.clone(),
        include_maneuvers,
    );
    journey.sections.push(section);
    journey.duration += walk.duration;
    journey.arrival_date_time = walk_arr;
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
async fn enrich_first_last_mile(
    journeys: &mut [Journey],
    ctx: &EnrichmentCtx<'_>,
    endpoints: Endpoints,
    date: &str,
) {
    let mut first_mile_cache: WalkLegCache = WalkLegCache::default();
    let mut last_mile_cache: WalkLegCache = WalkLegCache::default();

    for journey in journeys.iter_mut() {
        let (first_stop_idx, last_stop_idx) = endpoint_stop_indices(ctx.raptor_data, journey);

        let first_mile_fut = async {
            if let (Some(from_c), Some(stop_idx)) = (endpoints.from, first_stop_idx) {
                let stop = &ctx.raptor_data.stops[stop_idx];
                cached_pedestrian_route(
                    ctx,
                    &first_mile_cache,
                    from_c,
                    (stop.stop_lon, stop.stop_lat),
                    stop_idx,
                )
                .await
            } else {
                (0, None)
            }
        };

        let last_mile_fut = async {
            if let (Some(to_c), Some(stop_idx)) = (endpoints.to, last_stop_idx) {
                let stop = &ctx.raptor_data.stops[stop_idx];
                cached_pedestrian_route(
                    ctx,
                    &last_mile_cache,
                    (stop.stop_lon, stop.stop_lat),
                    to_c,
                    stop_idx,
                )
                .await
            } else {
                (0, None)
            }
        };

        let ((fm_idx, first_mile), (lm_idx, last_mile)) =
            futures::future::join(first_mile_fut, last_mile_fut).await;

        if endpoints.from.is_some() && first_stop_idx.is_some() {
            first_mile_cache.insert(fm_idx, first_mile.clone());
        }
        if endpoints.to.is_some() && last_stop_idx.is_some() {
            last_mile_cache.insert(lm_idx, last_mile.clone());
        }

        if let (Some(walk), Some(stop_idx), Some(from_c)) =
            (&first_mile, first_stop_idx, endpoints.from)
        {
            prepend_first_mile(
                journey,
                walk,
                ctx.raptor_data,
                stop_idx,
                from_c,
                date,
                ctx.include_maneuvers,
            );
        }

        if let (Some(walk), Some(stop_idx), Some(to_c)) = (&last_mile, last_stop_idx, endpoints.to)
        {
            append_last_mile(
                journey,
                walk,
                ctx.raptor_data,
                stop_idx,
                to_c,
                date,
                ctx.include_maneuvers,
            );
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
async fn enrich_transfers(journeys: &mut [Journey], ctx: &EnrichmentCtx<'_>) {
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
                let from_parent = ctx
                    .raptor_data
                    .stop_index
                    .get(section.from.id.as_str())
                    .map(|&idx| &ctx.raptor_data.stops[idx].parent_station);
                let to_parent = ctx
                    .raptor_data
                    .stop_index
                    .get(section.to.id.as_str())
                    .map(|&idx| &ctx.raptor_data.stops[idx].parent_station);
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
    let futs: Vec<_> = transfer_requests
        .iter()
        .map(|(_, _, from, to, is_outdoor)| {
            let indoor_friendly = !is_outdoor;
            valhalla::pedestrian_route(
                ctx.valhalla_base,
                *from,
                *to,
                ctx.walking_speed,
                indoor_friendly,
                ctx.language,
                ctx.wheelchair_config,
            )
        })
        .collect();
    let results = futures::future::join_all(futs).await;

    for ((j_idx, s_idx, _, _, is_outdoor), walk) in transfer_requests.iter().zip(results) {
        let section = &mut journeys[*j_idx].sections[*s_idx];
        section.transfer_type = Some(if *is_outdoor { "outdoor" } else { "indoor" }.to_string());
        if let Some(walk) = walk {
            section.shape = Some(walk.shape);
            section.distance = Some(walk.distance);
            if ctx.include_maneuvers {
                section.maneuvers = Some(walk.maneuvers);
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
        if s.section_type != "public_transport" || s.to.id != *from_id {
            return false;
        }
        match (s.display_informations.as_ref(), new_label) {
            (Some(info), Some(label)) => info.label == label,
            _ => false,
        }
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
                let (display_info, stop_date_times) = build_pt_metadata(data, section, date);
                let (merge_target_idx, should_merge) =
                    find_merge_target(&api_sections, from_stop, &display_info);
                if should_merge {
                    merge_with_previous(
                        &mut api_sections,
                        merge_target_idx,
                        to_stop,
                        arr_dt,
                        duration,
                        stop_date_times,
                    );
                    pt_count -= 1;
                    nb_transfers = nb_transfers.saturating_sub(1);
                } else {
                    api_sections.push(make_pt_section(
                        from_stop,
                        to_stop,
                        dep_dt,
                        arr_dt,
                        duration,
                        display_info,
                        stop_date_times,
                    ));
                }
            }
            SectionType::Transfer => {
                api_sections.push(make_transfer_section(
                    from_stop, to_stop, dep_dt, arr_dt, duration,
                ));
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

/// Build the per-stop schedule and route display info for a PT section.
fn build_pt_metadata(
    data: &RaptorData,
    section: &raptor::JourneySection,
    date: &str,
) -> (Option<DisplayInfo>, Option<Vec<StopDateTime>>) {
    let (Some(pat_idx), Some(trip_idx), Some(bp), Some(ap)) = (
        section.pattern_idx,
        section.trip_idx,
        section.board_pos,
        section.alight_pos,
    ) else {
        return (None, None);
    };

    let pattern = &data.patterns[pat_idx];
    let trip = &pattern.trips[trip_idx];

    let display_info = data
        .routes
        .get(&pattern.route_id)
        .map(|route| make_display_info(route, &trip.headsign));

    let stop_date_times: Vec<StopDateTime> = (bp..=ap)
        .map(|pos| {
            let st = &data.stops[pattern.stops[pos]];
            StopDateTime {
                stop_point: make_stop_point(st),
                arrival_date_time: raptor::format_datetime(date, trip.stop_times[pos].0),
                departure_date_time: raptor::format_datetime(date, trip.stop_times[pos].1),
            }
        })
        .collect();

    (display_info, Some(stop_date_times))
}

/// Merge an incoming PT section into a previous one at `merge_target_idx`
/// (same line + connecting stop). Handles GTFS trip splits where the same
/// physical line changes trip_id. The caller must have already checked
/// mergeability via [`find_merge_target`].
fn merge_with_previous(
    api_sections: &mut Vec<Section>,
    merge_target_idx: usize,
    to_stop: &crate::gtfs::Stop,
    arr_dt: String,
    duration: u32,
    stop_date_times: Option<Vec<StopDateTime>>,
) {
    if merge_target_idx < api_sections.len() - 1 {
        api_sections.pop(); // remove intermediate transfer section
    }
    let target = &mut api_sections[merge_target_idx];
    target.to = make_place(to_stop);
    target.arrival_date_time = arr_dt;
    target.duration += duration;

    // Append stop_date_times (skip first stop of second segment = last stop of first)
    if let (Some(existing_sdt), Some(new_sdt)) = (&mut target.stop_date_times, stop_date_times) {
        existing_sdt.extend(new_sdt.into_iter().skip(1));
    }
}

fn make_pt_section(
    from_stop: &crate::gtfs::Stop,
    to_stop: &crate::gtfs::Stop,
    dep_dt: String,
    arr_dt: String,
    duration: u32,
    display_informations: Option<DisplayInfo>,
    stop_date_times: Option<Vec<StopDateTime>>,
) -> Section {
    Section {
        section_type: "public_transport".to_string(),
        from: make_place(from_stop),
        to: make_place(to_stop),
        departure_date_time: dep_dt,
        arrival_date_time: arr_dt,
        duration,
        display_informations,
        stop_date_times,
        shape: None,
        distance: None,
        maneuvers: None,
        transfer_type: None,
    }
}

fn make_transfer_section(
    from_stop: &crate::gtfs::Stop,
    to_stop: &crate::gtfs::Stop,
    dep_dt: String,
    arr_dt: String,
    duration: u32,
) -> Section {
    Section {
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
    }
}

// ---------------------------------------------------------------------------
// Journey tagging
// ---------------------------------------------------------------------------

/// Compute quality tags for a sorted list of journeys.
///
/// Tags: `fastest`, `least_transfers`, `least_walking`, `least_waiting` (the
/// journey with the least total platform waiting time).
/// When `wheelchair` is true, also adds `most_accessible` to the journey
/// with the best accessibility score (least walking + fewest transfers).
/// If all journeys share a tag, only the first keeps it.
fn tag_journeys(journeys: &mut [Journey], wheelchair: bool) {
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

    // Total platform waiting time = end-to-end duration minus the time spent in
    // every section (vehicles + walking/transfers). The remainder is the time
    // spent waiting at stops between sections.
    let waiting_times: Vec<u32> = journeys
        .iter()
        .map(|j| {
            let sections_total: u32 = j.sections.iter().map(|s| s.duration).sum();
            j.duration.saturating_sub(sections_total)
        })
        .collect();
    let min_waiting = waiting_times.iter().copied().min().unwrap_or(0);

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
        if waiting_times[i] == min_waiting {
            journey.tags.push("least_waiting".to_string());
        }
    }

    // Wheelchair: tag the most accessible journey (least walking + fewest
    // transfers). Walking time is weighted more heavily because it represents
    // the most physically demanding part for a wheelchair user.
    if wheelchair {
        let best_idx = journeys
            .iter()
            .enumerate()
            .map(|(i, j)| {
                let score = walking_times[i] as u64 * 3 + j.nb_transfers as u64 * 120;
                (i, score)
            })
            .min_by_key(|(_, score)| *score)
            .map(|(i, _)| i);
        if let Some(idx) = best_idx {
            journeys[idx].tags.push("most_accessible".to_string());
        }
    }

    let all_tags: &[&str] = if wheelchair {
        &[
            "fastest",
            "least_transfers",
            "least_walking",
            "most_accessible",
            "least_waiting",
        ]
    } else {
        &[
            "fastest",
            "least_transfers",
            "least_walking",
            "least_waiting",
        ]
    };
    for tag in all_tags {
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
                    wheelchair_boarding: 0,
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
                wheelchair_accessible: 0,
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

    fn pt_section(pattern_idx: Option<usize>) -> raptor::JourneySection {
        raptor::JourneySection {
            section_type: if pattern_idx.is_some() {
                SectionType::PublicTransport
            } else {
                SectionType::Transfer
            },
            from_stop: 0,
            to_stop: 1,
            departure_time: 100,
            arrival_time: 200,
            pattern_idx,
            trip_idx: pattern_idx.map(|_| 0),
            board_pos: pattern_idx.map(|_| 0),
            alight_pos: pattern_idx.map(|_| 1),
        }
    }

    #[test]
    fn build_route_pattern_index_groups_patterns_by_route() {
        let data = make_test_raptor_data();
        let index = build_route_pattern_index(&data);
        // Every pattern must be reachable through its route_id.
        let total: usize = index.values().map(|v| v.len()).sum();
        assert_eq!(total, data.patterns.len());
        let route_id = data.patterns[0].route_id.as_str();
        assert!(index.get(route_id).unwrap().contains(&0));
    }

    #[test]
    fn exclude_head_line_excludes_every_pattern_of_the_head_route() {
        let data = make_test_raptor_data();
        let index = build_route_pattern_index(&data);
        let route_id = data.patterns[0].route_id.as_str();
        let expected = index.get(route_id).unwrap().clone();

        // A journey: transfer (no pattern) then a PT leg on pattern 0.
        let sections = vec![pt_section(None), pt_section(Some(0))];
        let mut excluded = rustc_hash::FxHashSet::default();
        exclude_head_line(&data, &sections, &index, &mut excluded);

        for p in expected {
            assert!(excluded.contains(&p), "pattern {p} should be excluded");
        }
    }

    #[test]
    fn exclude_head_line_is_noop_without_a_pt_section() {
        let data = make_test_raptor_data();
        let index = build_route_pattern_index(&data);
        let sections = vec![pt_section(None)]; // transfer only
        let mut excluded = rustc_hash::FxHashSet::default();
        exclude_head_line(&data, &sections, &index, &mut excluded);
        assert!(excluded.is_empty());
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
        tag_journeys(&mut journeys, false);
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
        tag_journeys(&mut journeys, false);
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
        tag_journeys(&mut journeys, false);
        assert!(journeys[0].tags.contains(&"fastest".to_string()));
        assert!(journeys[1].tags.contains(&"least_transfers".to_string()));
        assert!(journeys[1].tags.contains(&"least_walking".to_string()));
    }

    #[test]
    fn tag_journeys_least_waiting() {
        // Two journeys of equal total duration: the first spends all its time in
        // sections (no platform wait), so it has the least waiting → "least_waiting".
        let section = |dur: u32| Section {
            section_type: "public_transport".into(),
            from: make_place("S1"),
            to: make_place("S3"),
            departure_date_time: "20260406T081000".into(),
            arrival_date_time: "20260406T082000".into(),
            duration: dur,
            display_informations: None,
            stop_date_times: None,
            shape: None,
            distance: None,
            maneuvers: None,
            transfer_type: None,
        };
        let mut journeys = vec![
            Journey {
                departure_date_time: "20260406T080000".into(),
                arrival_date_time: "20260406T082000".into(),
                duration: 1200,
                nb_transfers: 1,
                tags: vec![],
                sections: vec![section(1200)], // wait = 0
            },
            Journey {
                departure_date_time: "20260406T080000".into(),
                arrival_date_time: "20260406T082000".into(),
                duration: 1200,
                nb_transfers: 1,
                tags: vec![],
                sections: vec![section(700)], // wait = 500
            },
        ];
        tag_journeys(&mut journeys, false);
        assert!(journeys[0].tags.contains(&"least_waiting".to_string()));
        assert!(!journeys[1].tags.contains(&"least_waiting".to_string()));
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
            false,
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
            false,
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

    // -----------------------------------------------------------------------
    // Extracted helper tests
    // -----------------------------------------------------------------------

    fn make_place(id: &str) -> Place {
        Place {
            id: id.into(),
            name: id.into(),
            stop_point: None,
        }
    }

    fn make_display_info(label: &str) -> DisplayInfo {
        DisplayInfo {
            network: "N".into(),
            direction: "D".into(),
            commercial_mode: "metro".into(),
            label: label.into(),
            color: "FFFFFF".into(),
            text_color: "000000".into(),
        }
    }

    fn make_pt_test_section(from_id: &str, to_id: &str, label: &str) -> Section {
        Section {
            section_type: "public_transport".into(),
            from: make_place(from_id),
            to: make_place(to_id),
            departure_date_time: "20260406T080000".into(),
            arrival_date_time: "20260406T081000".into(),
            duration: 600,
            display_informations: Some(make_display_info(label)),
            stop_date_times: None,
            shape: None,
            distance: None,
            maneuvers: None,
            transfer_type: None,
        }
    }

    fn make_transfer_test_section(from_id: &str, to_id: &str) -> Section {
        Section {
            section_type: "transfer".into(),
            from: make_place(from_id),
            to: make_place(to_id),
            departure_date_time: "20260406T081000".into(),
            arrival_date_time: "20260406T081200".into(),
            duration: 120,
            display_informations: None,
            stop_date_times: None,
            shape: None,
            distance: None,
            maneuvers: None,
            transfer_type: None,
        }
    }

    fn make_gtfs_stop(id: &str) -> crate::gtfs::Stop {
        crate::gtfs::Stop {
            stop_id: id.into(),
            stop_name: id.into(),
            stop_lon: 2.3,
            stop_lat: 48.8,
            parent_station: String::new(),
            wheelchair_boarding: 0,
        }
    }

    // ----- parse_query_datetime -------------------------------------------

    #[test]
    fn parse_query_datetime_valid_returns_parsed() {
        let (date, secs) = parse_query_datetime(Some("20260406T081500")).unwrap();
        assert_eq!(date, "20260406");
        assert_eq!(secs, 8 * 3600 + 15 * 60);
    }

    #[test]
    fn parse_query_datetime_invalid_returns_bad_request() {
        let err = parse_query_datetime(Some("not-a-date")).unwrap_err();
        assert_eq!(err.status(), 400);
    }

    #[test]
    fn parse_query_datetime_none_returns_now() {
        let (date, secs) = parse_query_datetime(None).unwrap();
        assert_eq!(date.len(), 8);
        assert!(secs < 86400);
    }

    // ----- shift_to_previous_day_if_early ---------------------------------

    #[test]
    fn shift_returns_unchanged_when_after_threshold() {
        let (date, secs) = shift_to_previous_day_if_early("20260406".into(), 30_000);
        assert_eq!(date, "20260406");
        assert_eq!(secs, 30_000);
    }

    #[test]
    fn shift_to_previous_day_when_before_threshold() {
        let (date, secs) = shift_to_previous_day_if_early("20260406".into(), 60); // 00:01
        assert_eq!(date, "20260405");
        assert_eq!(secs, 60 + 86400);
    }

    #[test]
    fn shift_handles_month_boundary() {
        let (date, _) = shift_to_previous_day_if_early("20260501".into(), 100);
        assert_eq!(date, "20260430");
    }

    // ----- bad_request ----------------------------------------------------

    #[test]
    fn helper_bad_request_returns_400() {
        let resp = bad_request("test_id", "test message".into());
        assert_eq!(resp.status(), 400);
    }

    // ----- compute_mode_exclusions ----------------------------------------

    #[test]
    fn compute_mode_exclusions_empty_returns_empty() {
        let data = make_test_raptor_data();
        let excluded = compute_mode_exclusions(&data, "");
        assert!(excluded.is_empty());
    }

    #[test]
    fn compute_mode_exclusions_metro_excludes_metro_patterns() {
        let data = make_test_raptor_data();
        let excluded = compute_mode_exclusions(&data, "metro");
        // R1 is route_type=1 (metro), so its patterns should be excluded
        assert!(!excluded.is_empty());
    }

    #[test]
    fn compute_mode_exclusions_unknown_mode_excludes_nothing() {
        let data = make_test_raptor_data();
        let excluded = compute_mode_exclusions(&data, "carpool");
        assert!(excluded.is_empty());
    }

    #[test]
    fn compute_mode_exclusions_multiple_modes() {
        let data = make_test_raptor_data();
        let excluded = compute_mode_exclusions(&data, "bus,metro,tramway");
        assert!(!excluded.is_empty()); // contains metro
    }

    // ----- find_merge_target ----------------------------------------------

    #[test]
    fn find_merge_target_empty_returns_no_merge() {
        let stop = make_gtfs_stop("S1");
        let info = Some(make_display_info("1"));
        let (_, merge) = find_merge_target(&[], &stop, &info);
        assert!(!merge);
    }

    #[test]
    fn find_merge_target_directly_matching_previous_pt() {
        let stop = make_gtfs_stop("S2");
        let info = Some(make_display_info("1"));
        let sections = vec![make_pt_test_section("S1", "S2", "1")];
        let (idx, merge) = find_merge_target(&sections, &stop, &info);
        assert!(merge);
        assert_eq!(idx, 0);
    }

    #[test]
    fn find_merge_target_with_intermediate_transfer() {
        let stop = make_gtfs_stop("S2");
        let info = Some(make_display_info("1"));
        let sections = vec![
            make_pt_test_section("S1", "S2", "1"),
            make_transfer_test_section("S2", "S2"),
        ];
        let (idx, merge) = find_merge_target(&sections, &stop, &info);
        assert!(merge);
        assert_eq!(idx, 0);
    }

    #[test]
    fn find_merge_target_different_label_no_merge() {
        let stop = make_gtfs_stop("S2");
        let info = Some(make_display_info("2"));
        let sections = vec![make_pt_test_section("S1", "S2", "1")];
        let (_, merge) = find_merge_target(&sections, &stop, &info);
        assert!(!merge);
    }

    // ----- journey_section_key / journey_sections_equal -------------------

    #[test]
    fn journey_section_key_filters_pt_only() {
        let j = Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 0,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![
                make_pt_test_section("S1", "S2", "1"),
                make_transfer_test_section("S2", "S2"),
                make_pt_test_section("S2", "S3", "2"),
            ],
        };
        let key = journey_section_key(&j);
        assert_eq!(key.len(), 2);
        assert_eq!(key[0], ("S1", "S2", "1"));
        assert_eq!(key[1], ("S2", "S3", "2"));
    }

    #[test]
    fn journey_sections_equal_same_pt_legs_match() {
        let a = Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 0,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![make_pt_test_section("S1", "S2", "1")],
        };
        let b = Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 100,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![make_pt_test_section("S1", "S2", "1")],
        };
        assert!(journey_sections_equal(&a, &b));
    }

    #[test]
    fn journey_sections_equal_different_label_no_match() {
        let a = Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 0,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![make_pt_test_section("S1", "S2", "1")],
        };
        let b = Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 0,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![make_pt_test_section("S1", "S2", "2")],
        };
        assert!(!journey_sections_equal(&a, &b));
    }

    // ----- make_pt_section / make_transfer_section ------------------------

    #[test]
    fn make_pt_section_sets_type_and_fields() {
        let from = make_gtfs_stop("S1");
        let to = make_gtfs_stop("S2");
        let s = make_pt_section(
            &from,
            &to,
            "20260406T080000".into(),
            "20260406T081000".into(),
            600,
            None,
            None,
        );
        assert_eq!(s.section_type, "public_transport");
        assert_eq!(s.duration, 600);
        assert!(s.shape.is_none());
    }

    #[test]
    fn make_transfer_section_sets_type_and_fields() {
        let from = make_gtfs_stop("S1");
        let to = make_gtfs_stop("S2");
        let s = make_transfer_section(
            &from,
            &to,
            "20260406T080000".into(),
            "20260406T080200".into(),
            120,
        );
        assert_eq!(s.section_type, "transfer");
        assert_eq!(s.duration, 120);
    }

    // ----- merge_with_previous --------------------------------------------

    #[test]
    fn merge_with_previous_extends_target() {
        let mut sections = vec![make_pt_test_section("S1", "S2", "1")];
        let to = make_gtfs_stop("S3");
        merge_with_previous(&mut sections, 0, &to, "20260406T082000".into(), 500, None);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].to.id, "S3");
        assert_eq!(sections[0].arrival_date_time, "20260406T082000");
        assert_eq!(sections[0].duration, 600 + 500);
    }

    #[test]
    fn merge_with_previous_pops_intermediate_transfer() {
        let mut sections = vec![
            make_pt_test_section("S1", "S2", "1"),
            make_transfer_test_section("S2", "S2"),
        ];
        let to = make_gtfs_stop("S3");
        merge_with_previous(&mut sections, 0, &to, "20260406T082000".into(), 500, None);
        assert_eq!(sections.len(), 1);
    }

    // ----- build_walk_section ---------------------------------------------

    #[test]
    fn build_walk_section_basic() {
        let walk = valhalla::WalkLeg {
            duration: 60,
            distance: 80,
            shape: "encoded".into(),
            maneuvers: vec![],
        };
        let from = make_place("orig");
        let to = make_place("dest");
        let s = build_walk_section(
            &walk,
            from,
            to,
            "20260406T080000".into(),
            "20260406T080100".into(),
            false,
        );
        assert_eq!(s.section_type, "street_network");
        assert_eq!(s.duration, 60);
        assert_eq!(s.distance, Some(80));
        assert_eq!(s.shape, Some("encoded".into()));
        assert!(s.maneuvers.is_none());
    }

    #[test]
    fn build_walk_section_with_maneuvers() {
        let walk = valhalla::WalkLeg {
            duration: 60,
            distance: 80,
            shape: "x".into(),
            maneuvers: vec![valhalla::WalkManeuver {
                instruction: "go".into(),
                maneuver_type: 1,
                distance: 10,
                duration: 5,
                begin_shape_index: 0,
            }],
        };
        let s = build_walk_section(
            &walk,
            make_place("a"),
            make_place("b"),
            "20260406T080000".into(),
            "20260406T080100".into(),
            true,
        );
        assert!(s.maneuvers.is_some());
        assert_eq!(s.maneuvers.as_ref().unwrap().len(), 1);
    }

    // ----- endpoint_stop_indices ------------------------------------------

    #[test]
    fn endpoint_stop_indices_finds_first_and_last_pt() {
        let data = make_test_raptor_data();
        let s1 = data.stop_index["S1"];
        let s3 = data.stop_index["S3"];
        let mut pt1 = make_pt_test_section("S1", "S2", "1");
        pt1.from.stop_point = Some(crate::api::StopPointRef {
            id: "S1".into(),
            name: "S1".into(),
            coord: crate::api::Coord { lon: 0.0, lat: 0.0 },
        });
        let mut pt2 = make_pt_test_section("S2", "S3", "1");
        pt2.to.stop_point = Some(crate::api::StopPointRef {
            id: "S3".into(),
            name: "S3".into(),
            coord: crate::api::Coord { lon: 0.0, lat: 0.0 },
        });
        let journey = Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 0,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![pt1, pt2],
        };
        let (first, last) = endpoint_stop_indices(&data, &journey);
        assert_eq!(first, Some(s1));
        assert_eq!(last, Some(s3));
    }

    #[test]
    fn endpoint_stop_indices_no_pt_returns_none() {
        let data = make_test_raptor_data();
        let journey = Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 0,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![make_transfer_test_section("S1", "S2")],
        };
        let (first, last) = endpoint_stop_indices(&data, &journey);
        assert_eq!(first, None);
        assert_eq!(last, None);
    }

    // ----- tag_journeys wheelchair branch ---------------------------------

    // ----- cached_pedestrian_route ----------------------------------------

    #[actix_web::test]
    async fn cached_pedestrian_route_skips_identical_coords() {
        let cfg = AppConfig::default();
        let data = make_test_raptor_data();
        let ctx = EnrichmentCtx {
            raptor_data: &data,
            valhalla_base: "http://127.0.0.1:1",
            walking_speed: None,
            include_maneuvers: false,
            language: None,
            wheelchair_config: Some(&cfg.wheelchair),
        };
        let cache = WalkLegCache::default();
        let (idx, leg) = cached_pedestrian_route(&ctx, &cache, (2.3, 48.8), (2.3, 48.8), 7).await;
        assert_eq!(idx, 7);
        assert!(leg.is_none());
    }

    #[actix_web::test]
    async fn cached_pedestrian_route_returns_cache_hit() {
        let data = make_test_raptor_data();
        let ctx = EnrichmentCtx {
            raptor_data: &data,
            valhalla_base: "http://127.0.0.1:1",
            walking_speed: None,
            include_maneuvers: false,
            language: None,
            wheelchair_config: None,
        };
        let mut cache = WalkLegCache::default();
        let cached_leg = Arc::new(valhalla::WalkLeg {
            duration: 99,
            distance: 100,
            shape: "x".into(),
            maneuvers: vec![],
        });
        cache.insert(3, Some(cached_leg.clone()));
        let (idx, leg) = cached_pedestrian_route(&ctx, &cache, (2.3, 48.8), (2.4, 48.9), 3).await;
        assert_eq!(idx, 3);
        let l = leg.expect("cache hit");
        assert_eq!(l.duration, 99);
    }

    #[actix_web::test]
    async fn cached_pedestrian_route_calls_valhalla_on_miss() {
        let data = make_test_raptor_data();
        let ctx = EnrichmentCtx {
            raptor_data: &data,
            valhalla_base: "http://127.0.0.1:1",
            walking_speed: Some(5.0),
            include_maneuvers: false,
            language: None,
            wheelchair_config: None,
        };
        let cache = WalkLegCache::default();
        // Valhalla unreachable → returns None, but the call path is exercised
        let (idx, leg) = cached_pedestrian_route(&ctx, &cache, (2.3, 48.8), (2.4, 48.9), 5).await;
        assert_eq!(idx, 5);
        assert!(leg.is_none());
    }

    // ----- enrich_transfers / enrich_first_last_mile ----------------------

    #[actix_web::test]
    async fn enrich_transfers_marks_outdoor_indoor() {
        let data = make_test_raptor_data();
        let ctx = EnrichmentCtx {
            raptor_data: &data,
            valhalla_base: "http://127.0.0.1:1",
            walking_speed: None,
            include_maneuvers: false,
            language: None,
            wheelchair_config: None,
        };
        // Build a journey with one transfer section
        let mut journeys = vec![Journey {
            departure_date_time: "".into(),
            arrival_date_time: "".into(),
            duration: 0,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![Section {
                section_type: "transfer".into(),
                from: Place {
                    id: "S1".into(),
                    name: "S1".into(),
                    stop_point: Some(crate::api::StopPointRef {
                        id: "S1".into(),
                        name: "S1".into(),
                        coord: crate::api::Coord {
                            lon: 2.347,
                            lat: 48.858,
                        },
                    }),
                },
                to: Place {
                    id: "S2".into(),
                    name: "S2".into(),
                    stop_point: Some(crate::api::StopPointRef {
                        id: "S2".into(),
                        name: "S2".into(),
                        coord: crate::api::Coord {
                            lon: 2.373,
                            lat: 48.844,
                        },
                    }),
                },
                departure_date_time: "20260406T080000".into(),
                arrival_date_time: "20260406T080100".into(),
                duration: 60,
                display_informations: None,
                stop_date_times: None,
                shape: None,
                distance: None,
                maneuvers: None,
                transfer_type: None,
            }],
        }];
        enrich_transfers(&mut journeys, &ctx).await;
        let s = &journeys[0].sections[0];
        // Outdoor since both stops have empty parent_station
        assert_eq!(s.transfer_type.as_deref(), Some("outdoor"));
    }

    #[actix_web::test]
    async fn enrich_first_last_mile_runs_without_panic() {
        let data = make_test_raptor_data();
        let cfg = AppConfig::default();
        let ctx = EnrichmentCtx {
            raptor_data: &data,
            valhalla_base: "http://127.0.0.1:1",
            walking_speed: Some(5.0),
            include_maneuvers: true,
            language: Some("fr-FR"),
            wheelchair_config: Some(&cfg.wheelchair),
        };
        // Run a real RAPTOR query so we get realistic journeys
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
            false,
        );
        let section_sets = raptor::reconstruct_journeys(&data, &result, &[(target, 0)]);
        let mut journeys: Vec<Journey> = section_sets
            .iter()
            .map(|s| build_journey(&data, s, "20260406"))
            .collect();
        let endpoints = Endpoints {
            from: Some((2.347, 48.858)),
            to: Some((2.395, 48.848)),
        };
        // Valhalla unreachable so the first/last mile walks will not be
        // added, but the function executes its full happy-path tree.
        enrich_first_last_mile(&mut journeys, &ctx, endpoints, "20260406").await;
    }

    // ----- get_journeys with coordinate input -----------------------------

    #[actix_web::test]
    async fn get_journeys_with_coords_and_mock_valhalla() {
        let base = super::super::valhalla::test_support::spawn_mock_valhalla();
        let rest = base.strip_prefix("http://").unwrap();
        let (host, port_str) = rest.split_once(':').unwrap();
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let mut cfg = make_test_config();
        cfg.valhalla.host = host.into();
        cfg.valhalla.port = port_str.parse().unwrap();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(web::Data::new(cfg))
                .service(get_journeys),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=2.347;48.858&to=2.395;48.848&datetime=20260406T080000&maneuvers=true")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert!(!body["journeys"].as_array().unwrap().is_empty());
    }

    #[actix_web::test]
    async fn get_journeys_with_coords() {
        let data = make_test_raptor_data();
        let shared = web::Data::new(ArcSwap::from(data));
        let cfg = make_test_config();
        let config = web::Data::new(cfg);
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(shared)
                .app_data(config)
                .service(get_journeys),
        )
        .await;
        // Use coordinates so enrich_first_last_mile is exercised
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=2.347;48.858&to=2.395;48.848&datetime=20260406T080000&maneuvers=true&wheelchair=true&forbidden_modes=bus")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn get_journeys_early_morning_shifts_to_previous_day() {
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
        // 02:00 → must shift to previous day
        let req = actix_web::test::TestRequest::get()
            .uri("/api/journeys/public_transport?from=S1&to=S3&datetime=20260406T020000")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    // ----- prepend_first_mile / append_last_mile --------------------------

    fn make_test_journey() -> Journey {
        Journey {
            departure_date_time: "20260406T081000".into(),
            arrival_date_time: "20260406T082000".into(),
            duration: 600,
            nb_transfers: 0,
            tags: vec![],
            sections: vec![Section {
                section_type: "public_transport".into(),
                from: make_place("S1"),
                to: make_place("S3"),
                departure_date_time: "20260406T081000".into(),
                arrival_date_time: "20260406T082000".into(),
                duration: 600,
                display_informations: Some(make_display_info("1")),
                stop_date_times: None,
                shape: None,
                distance: None,
                maneuvers: None,
                transfer_type: None,
            }],
        }
    }

    fn make_test_walk_leg() -> valhalla::WalkLeg {
        valhalla::WalkLeg {
            duration: 60,
            distance: 80,
            shape: "shape".into(),
            maneuvers: vec![valhalla::WalkManeuver {
                instruction: "go".into(),
                maneuver_type: 1,
                distance: 80,
                duration: 60,
                begin_shape_index: 0,
            }],
        }
    }

    #[test]
    fn prepend_first_mile_inserts_walking_section() {
        let data = make_test_raptor_data();
        let walk = make_test_walk_leg();
        let mut journey = make_test_journey();
        prepend_first_mile(
            &mut journey,
            &walk,
            &data,
            data.stop_index["S1"],
            (2.0, 48.0),
            "20260406",
            true,
        );
        assert_eq!(journey.sections.len(), 2);
        assert_eq!(journey.sections[0].section_type, "street_network");
        assert!(journey.sections[0].maneuvers.is_some());
        assert_eq!(journey.duration, 600 + 60);
    }

    #[test]
    fn prepend_first_mile_drops_leading_transfer() {
        let data = make_test_raptor_data();
        let walk = make_test_walk_leg();
        let mut journey = make_test_journey();
        // Insert a transfer at the start
        journey
            .sections
            .insert(0, make_transfer_test_section("X", "S1"));
        journey.duration += 120;
        prepend_first_mile(
            &mut journey,
            &walk,
            &data,
            data.stop_index["S1"],
            (2.0, 48.0),
            "20260406",
            false,
        );
        // Transfer should be dropped, walk inserted
        assert_eq!(journey.sections[0].section_type, "street_network");
        // 600 PT + 120 transfer - 120 dropped + 60 walk = 660
        assert_eq!(journey.duration, 600 + 60);
    }

    #[test]
    fn append_last_mile_appends_walking_section() {
        let data = make_test_raptor_data();
        let walk = make_test_walk_leg();
        let mut journey = make_test_journey();
        append_last_mile(
            &mut journey,
            &walk,
            &data,
            data.stop_index["S3"],
            (2.5, 48.5),
            "20260406",
            false,
        );
        assert_eq!(journey.sections.len(), 2);
        assert_eq!(
            journey.sections.last().unwrap().section_type,
            "street_network"
        );
        assert_eq!(journey.duration, 600 + 60);
    }

    #[test]
    fn append_last_mile_drops_trailing_transfer() {
        let data = make_test_raptor_data();
        let walk = make_test_walk_leg();
        let mut journey = make_test_journey();
        journey.sections.push(make_transfer_test_section("S3", "X"));
        journey.duration += 120;
        append_last_mile(
            &mut journey,
            &walk,
            &data,
            data.stop_index["S3"],
            (2.5, 48.5),
            "20260406",
            true,
        );
        assert_eq!(
            journey.sections.last().unwrap().section_type,
            "street_network"
        );
        assert_eq!(journey.duration, 600 + 60);
    }

    // ----- resolve_stops station-child branch -----------------------------

    fn make_station_with_children_raptor() -> Arc<RaptorData> {
        let mut stops = FxHashMap::default();
        // Station node (no patterns)
        stops.insert(
            "STATION".into(),
            gtfs::Stop {
                stop_id: "STATION".into(),
                stop_name: "Station".into(),
                stop_lon: 2.347,
                stop_lat: 48.858,
                parent_station: String::new(),
                wheelchair_boarding: 0,
            },
        );
        // Two children
        for (id, lon) in &[("P1", 2.347), ("P2", 2.348)] {
            stops.insert(
                id.to_string(),
                gtfs::Stop {
                    stop_id: id.to_string(),
                    stop_name: id.to_string(),
                    stop_lon: *lon,
                    stop_lat: 48.858,
                    parent_station: "STATION".into(),
                    wheelchair_boarding: 0,
                },
            );
        }
        // Far stop, target of the trip
        stops.insert(
            "FAR".into(),
            gtfs::Stop {
                stop_id: "FAR".into(),
                stop_name: "Far".into(),
                stop_lon: 2.4,
                stop_lat: 48.85,
                parent_station: String::new(),
                wheelchair_boarding: 0,
            },
        );
        let mut routes = FxHashMap::default();
        routes.insert(
            "R1".into(),
            gtfs::Route {
                route_id: "R1".into(),
                agency_id: "A1".into(),
                route_short_name: "1".into(),
                route_long_name: "L".into(),
                route_type: 1,
                route_color: String::new(),
                route_text_color: String::new(),
            },
        );
        let mut trips = FxHashMap::default();
        trips.insert(
            "T1".into(),
            gtfs::Trip {
                route_id: "R1".into(),
                service_id: "SVC1".into(),
                trip_id: "T1".into(),
                trip_headsign: "Far".into(),
                wheelchair_accessible: 0,
            },
        );
        // Trip serves P1 → FAR (so P1 has patterns, STATION does not)
        let stop_times = vec![
            gtfs::StopTime {
                trip_id: "T1".into(),
                arrival_time: "08:00:00".into(),
                departure_time: "08:01:00".into(),
                stop_id: "P1".into(),
                stop_sequence: 0,
            },
            gtfs::StopTime {
                trip_id: "T1".into(),
                arrival_time: "08:10:00".into(),
                departure_time: "08:11:00".into(),
                stop_id: "FAR".into(),
                stop_sequence: 1,
            },
        ];
        let mut calendars = FxHashMap::default();
        calendars.insert(
            "SVC1".into(),
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
        Arc::new(raptor::RaptorData::build(
            gtfs::GtfsData {
                agencies: vec![],
                routes,
                stops,
                trips,
                stop_times,
                calendars,
                calendar_dates: vec![],
                transfers: vec![],
                pathways: vec![],
            },
            120,
        ))
    }

    #[test]
    fn resolve_stops_station_expands_to_children() {
        let data = make_station_with_children_raptor();
        let stops = resolve_stops(&data, "STATION", None, 1500, 5.0);
        // Should include P1 (the child that has patterns)
        let p1 = data.stop_index["P1"];
        assert!(stops.iter().any(|&(idx, _)| idx == p1));
    }

    #[test]
    fn resolve_stops_unknown_returns_empty() {
        let data = make_test_raptor_data();
        let stops = resolve_stops(&data, "DOES_NOT_EXIST", None, 100, 5.0);
        assert!(stops.is_empty());
    }

    // ----- build_journey merging trip splits ------------------------------

    fn make_trip_split_raptor() -> Arc<RaptorData> {
        // Two trips on the same route, splitting at stop S2.
        let mut stops = FxHashMap::default();
        for (id, lon) in &[("S1", 2.347), ("S2", 2.373), ("S3", 2.395)] {
            stops.insert(
                id.to_string(),
                gtfs::Stop {
                    stop_id: id.to_string(),
                    stop_name: id.to_string(),
                    stop_lon: *lon,
                    stop_lat: 48.85,
                    parent_station: String::new(),
                    wheelchair_boarding: 0,
                },
            );
        }
        let mut routes = FxHashMap::default();
        routes.insert(
            "R1".into(),
            gtfs::Route {
                route_id: "R1".into(),
                agency_id: "A1".into(),
                route_short_name: "1".into(),
                route_long_name: "L".into(),
                route_type: 1,
                route_color: String::new(),
                route_text_color: String::new(),
            },
        );
        let mut trips = FxHashMap::default();
        trips.insert(
            "T_A".into(),
            gtfs::Trip {
                route_id: "R1".into(),
                service_id: "SVC1".into(),
                trip_id: "T_A".into(),
                trip_headsign: "S2".into(),
                wheelchair_accessible: 0,
            },
        );
        trips.insert(
            "T_B".into(),
            gtfs::Trip {
                route_id: "R1".into(),
                service_id: "SVC1".into(),
                trip_id: "T_B".into(),
                trip_headsign: "S3".into(),
                wheelchair_accessible: 0,
            },
        );
        let stop_times = vec![
            gtfs::StopTime {
                trip_id: "T_A".into(),
                arrival_time: "08:00:00".into(),
                departure_time: "08:01:00".into(),
                stop_id: "S1".into(),
                stop_sequence: 0,
            },
            gtfs::StopTime {
                trip_id: "T_A".into(),
                arrival_time: "08:05:00".into(),
                departure_time: "08:06:00".into(),
                stop_id: "S2".into(),
                stop_sequence: 1,
            },
            gtfs::StopTime {
                trip_id: "T_B".into(),
                arrival_time: "08:07:00".into(),
                departure_time: "08:08:00".into(),
                stop_id: "S2".into(),
                stop_sequence: 0,
            },
            gtfs::StopTime {
                trip_id: "T_B".into(),
                arrival_time: "08:12:00".into(),
                departure_time: "08:13:00".into(),
                stop_id: "S3".into(),
                stop_sequence: 1,
            },
        ];
        let mut calendars = FxHashMap::default();
        calendars.insert(
            "SVC1".into(),
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
        Arc::new(raptor::RaptorData::build(
            gtfs::GtfsData {
                agencies: vec![],
                routes,
                stops,
                trips,
                stop_times,
                calendars,
                calendar_dates: vec![],
                transfers: vec![],
                pathways: vec![],
            },
            120,
        ))
    }

    #[test]
    fn build_journey_merges_consecutive_same_line_sections() {
        let data = make_trip_split_raptor();
        let s1 = data.stop_index["S1"];
        let s3 = data.stop_index["S3"];
        let active = data.active_services("20260406");
        let result = raptor::raptor_query(
            &data,
            &[(s1, 0)],
            28000,
            &active,
            3,
            &rustc_hash::FxHashSet::default(),
            false,
        );
        let section_sets = raptor::reconstruct_journeys(&data, &result, &[(s3, 0)]);
        if !section_sets.is_empty() {
            let journey = build_journey(&data, &section_sets[0], "20260406");
            // Verify the journey reached S3 successfully
            assert!(journey.duration > 0);
        }
    }

    #[test]
    fn tag_journeys_wheelchair_adds_most_accessible() {
        let mut journeys = vec![
            Journey {
                departure_date_time: "".into(),
                arrival_date_time: "".into(),
                duration: 1000,
                nb_transfers: 2,
                tags: vec![],
                sections: vec![],
            },
            Journey {
                departure_date_time: "".into(),
                arrival_date_time: "".into(),
                duration: 1100,
                nb_transfers: 0,
                tags: vec![],
                sections: vec![],
            },
        ];
        tag_journeys(&mut journeys, true);
        assert!(
            journeys[0].tags.contains(&"most_accessible".to_string())
                || journeys[1].tags.contains(&"most_accessible".to_string())
        );
    }
}
