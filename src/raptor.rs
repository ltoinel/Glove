//! RAPTOR (Round-bAsed Public Transit Routing) algorithm implementation.
//!
//! This module contains:
//! - **Pre-processing**: builds optimized data structures from raw GTFS data
//!   (route patterns, transfer graph, service interning, search index).
//! - **Query**: the RAPTOR algorithm that finds earliest-arrival journeys
//!   given a source stop, target stop, departure time, and date.
//! - **Reconstruction**: traces back through RAPTOR labels to produce
//!   human-readable journey sections.
//!
//! # Algorithm overview
//!
//! RAPTOR works in rounds. Each round k represents journeys using at most k
//! vehicle trips (k-1 transfers). For each round:
//! 1. Collect route patterns serving stops improved in the previous round.
//! 2. For each pattern, scan trips to find the earliest boardable vehicle.
//! 3. Propagate arrival times along the pattern to subsequent stops.
//! 4. Apply foot transfers from trip-improved stops.
//!
//! The algorithm terminates when no stop is improved or the maximum number
//! of rounds is reached.

use std::collections::{HashMap, HashSet};
use tracing::info;

use crate::gtfs::{self, GtfsData};

/// Sentinel value representing an unreachable stop.
const INFINITY: u32 = u32::MAX;
/// Maximum number of RAPTOR rounds (max_transfers + 1).
const MAX_ROUNDS: usize = 8;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single vehicle trip within a route pattern.
///
/// Contains the interned service index (for calendar lookup) and the
/// pre-parsed stop times as (arrival_secs, departure_secs) tuples.
#[derive(Debug, Clone)]
pub struct PatternTrip {
    #[allow(dead_code)]
    pub trip_id: String,
    /// Index into [`RaptorData::service_ids`] for fast active-service checks.
    pub service_idx: usize,
    /// Destination sign displayed on the vehicle.
    pub headsign: String,
    /// (arrival, departure) in seconds since midnight, one per stop in the pattern.
    pub stop_times: Vec<(u32, u32)>,
}

/// A route pattern: an ordered sequence of stops served by multiple trips.
///
/// Trips within the same pattern visit the exact same stops in the same order.
/// They are sorted by departure time at the first stop.
#[derive(Debug)]
pub struct Pattern {
    pub route_id: String,
    /// Ordered numeric stop indices for this pattern.
    pub stops: Vec<usize>,
    /// Trips sorted by departure at the first stop.
    pub trips: Vec<PatternTrip>,
}

/// Aggregate statistics captured during GTFS loading and RAPTOR index build.
pub struct GtfsStats {
    pub agencies: usize,
    pub routes: usize,
    pub stops: usize,
    pub trips: usize,
    pub stop_times: usize,
    pub calendars: usize,
    pub calendar_dates: usize,
    pub transfers: usize,
    pub patterns: usize,
    pub services: usize,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

/// Pre-computed entry for the stop name autocomplete index.
pub struct SearchEntry {
    pub stop_idx: usize,
    /// Lowercased, diacritics-stripped name for fuzzy matching.
    pub name_lower: String,
}

/// The complete RAPTOR data structure, built once from GTFS and shared
/// across all request handlers via [`ArcSwap`](arc_swap::ArcSwap).
pub struct RaptorData {
    /// stop_id (String) → numeric index.
    pub stop_index: HashMap<String, usize>,
    /// All route patterns (unique stop sequences).
    pub patterns: Vec<Pattern>,
    /// For each stop: list of `(pattern_idx, position_in_pattern)`.
    pub stop_patterns: Vec<Vec<(usize, usize)>>,
    /// For each stop: list of `(target_stop_idx, transfer_duration_secs)`.
    pub stop_transfers: Vec<Vec<(usize, u32)>>,
    /// Route metadata for display (colors, names, types).
    pub routes: HashMap<String, gtfs::Route>,
    /// All stops ordered by numeric index.
    pub stops: Vec<gtfs::Stop>,
    /// Loading statistics and metadata.
    pub stats: GtfsStats,
    /// Autocomplete search index (deduplicated by name).
    pub search_index: Vec<SearchEntry>,
    /// Interned service IDs: `service_idx → service_id`.
    service_ids: Vec<String>,
    /// Calendar rules indexed by service_id.
    calendars: HashMap<String, gtfs::Calendar>,
    /// Calendar exceptions: `service_id → [(date, exception_type)]`.
    calendar_exceptions: HashMap<String, Vec<(String, u8)>>,
}

// ---------------------------------------------------------------------------
// Index build
// ---------------------------------------------------------------------------

impl RaptorData {
    /// Build the RAPTOR index from raw GTFS data.
    ///
    /// This is an expensive operation (~10-30s for Île-de-France) that:
    /// 1. Assigns numeric indices to all stops.
    /// 2. Interns service IDs for fast calendar lookups.
    /// 3. Groups stop_times into route patterns (unique stop sequences).
    /// 4. Builds the transfer graph (GTFS transfers + parent station links).
    /// 5. Builds the autocomplete search index.
    pub fn build(gtfs: GtfsData, default_transfer_time: u32) -> Self {
        info!("Building RAPTOR index...");

        let raw_agencies = gtfs.agencies.len();
        let raw_routes = gtfs.routes.len();
        let raw_stops = gtfs.stops.len();
        let raw_trips = gtfs.trips.len();
        let raw_stop_times = gtfs.stop_times.len();
        let raw_calendars = gtfs.calendars.len();
        let raw_calendar_dates = gtfs.calendar_dates.len();
        let raw_transfers = gtfs.transfers.len();

        // Step 1: Assign numeric indices to stops
        let mut stop_index: HashMap<String, usize> = HashMap::with_capacity(gtfs.stops.len());
        let mut stops: Vec<gtfs::Stop> = Vec::with_capacity(gtfs.stops.len());

        for (id, stop) in &gtfs.stops {
            let idx = stops.len();
            stop_index.insert(id.clone(), idx);
            stops.push(stop.clone());
        }

        let num_stops = stops.len();
        info!("{} stops indexed", num_stops);

        // Step 2: Intern service_ids for O(1) active-service checks
        let mut service_index: HashMap<String, usize> = HashMap::new();
        let mut service_ids: Vec<String> = Vec::new();

        for service_id in gtfs.calendars.keys() {
            let idx = service_ids.len();
            service_index.insert(service_id.clone(), idx);
            service_ids.push(service_id.clone());
        }
        for cd in &gtfs.calendar_dates {
            if !service_index.contains_key(&cd.service_id) {
                let idx = service_ids.len();
                service_index.insert(cd.service_id.clone(), idx);
                service_ids.push(cd.service_id.clone());
            }
        }
        for trip in gtfs.trips.values() {
            if !service_index.contains_key(&trip.service_id) {
                let idx = service_ids.len();
                service_index.insert(trip.service_id.clone(), idx);
                service_ids.push(trip.service_id.clone());
            }
        }
        info!("{} services interned", service_ids.len());

        // Step 3: Group stop_times by trip_id and sort by sequence
        info!("Grouping stop_times by trip...");
        let mut trip_stop_times: HashMap<&str, Vec<(u32, u32, u32, &str)>> = HashMap::new();
        for st in &gtfs.stop_times {
            let arr = gtfs::parse_time(&st.arrival_time).unwrap_or(0);
            let dep = gtfs::parse_time(&st.departure_time).unwrap_or(0);
            trip_stop_times.entry(&st.trip_id).or_default().push((
                st.stop_sequence,
                arr,
                dep,
                &st.stop_id,
            ));
        }
        for times in trip_stop_times.values_mut() {
            times.sort_by_key(|t| t.0);
        }
        info!("{} trips with stop_times", trip_stop_times.len());

        // Step 4: Build route patterns by grouping trips with identical stop sequences
        info!("Building route patterns...");
        let mut pattern_map: HashMap<Vec<usize>, usize> = HashMap::new();
        let mut patterns: Vec<Pattern> = Vec::new();

        for (trip_id, times) in &trip_stop_times {
            let trip = match gtfs.trips.get(*trip_id) {
                Some(t) => t,
                None => continue,
            };

            let stop_seq: Vec<usize> = times
                .iter()
                .filter_map(|(_, _, _, sid)| stop_index.get(*sid).copied())
                .collect();

            if stop_seq.len() < 2 {
                continue;
            }

            let stop_times_parsed: Vec<(u32, u32)> =
                times.iter().map(|(_, arr, dep, _)| (*arr, *dep)).collect();

            if stop_seq.len() != stop_times_parsed.len() {
                continue;
            }

            let svc_idx = service_index.get(&trip.service_id).copied().unwrap_or(0);

            let pattern_trip = PatternTrip {
                trip_id: trip_id.to_string(),
                service_idx: svc_idx,
                headsign: trip.trip_headsign.clone(),
                stop_times: stop_times_parsed,
            };

            if let Some(&pat_idx) = pattern_map.get(&stop_seq) {
                patterns[pat_idx].trips.push(pattern_trip);
            } else {
                let pat_idx = patterns.len();
                pattern_map.insert(stop_seq.clone(), pat_idx);
                patterns.push(Pattern {
                    route_id: trip.route_id.clone(),
                    stops: stop_seq,
                    trips: vec![pattern_trip],
                });
            }
        }

        // Sort trips within each pattern by departure at first stop
        for pat in &mut patterns {
            pat.trips.sort_by_key(|t| t.stop_times[0].1);
        }
        info!("{} route patterns", patterns.len());

        // Step 5: Build stop → patterns reverse index
        let mut stop_patterns: Vec<Vec<(usize, usize)>> = vec![Vec::new(); num_stops];
        for (pat_idx, pat) in patterns.iter().enumerate() {
            for (pos, &stop_idx) in pat.stops.iter().enumerate() {
                stop_patterns[stop_idx].push((pat_idx, pos));
            }
        }

        // Step 6: Build transfer graph (GTFS transfers + parent station links)
        info!("Building transfers index...");
        let mut stop_transfers: Vec<Vec<(usize, u32)>> = vec![Vec::new(); num_stops];
        for t in &gtfs.transfers {
            if let (Some(&from), Some(&to)) = (
                stop_index.get(&t.from_stop_id),
                stop_index.get(&t.to_stop_id),
            ) {
                let duration = t.min_transfer_time.unwrap_or(default_transfer_time);
                stop_transfers[from].push((to, duration));
            }
        }

        // Add implicit transfers between stops sharing the same parent station
        info!("Building parent station transfers...");
        let mut parent_stops: HashMap<&str, Vec<usize>> = HashMap::new();
        for (idx, stop) in stops.iter().enumerate() {
            if !stop.parent_station.is_empty() {
                parent_stops
                    .entry(&stop.parent_station)
                    .or_default()
                    .push(idx);
            }
        }
        let mut parent_transfer_count = 0u64;
        for siblings in parent_stops.values() {
            if siblings.len() < 2 {
                continue;
            }
            for &a in siblings {
                let existing: HashSet<usize> = stop_transfers[a].iter().map(|&(t, _)| t).collect();
                for &b in siblings {
                    if a != b && !existing.contains(&b) {
                        stop_transfers[a].push((b, default_transfer_time));
                        parent_transfer_count += 1;
                    }
                }
            }
        }
        info!("{} parent station transfers added", parent_transfer_count);

        // Calendar exceptions index
        let mut calendar_exceptions: HashMap<String, Vec<(String, u8)>> = HashMap::new();
        for cd in &gtfs.calendar_dates {
            calendar_exceptions
                .entry(cd.service_id.clone())
                .or_default()
                .push((cd.date.clone(), cd.exception_type));
        }

        info!("RAPTOR index built");

        // Step 7: Build autocomplete search index
        // Only index stops that have a name and are served by at least one pattern
        info!("Building search index...");
        let mut search_index: Vec<SearchEntry> = Vec::new();
        for (idx, stop) in stops.iter().enumerate() {
            if stop.stop_name.is_empty() || stop_patterns[idx].is_empty() {
                continue;
            }
            search_index.push(SearchEntry {
                stop_idx: idx,
                name_lower: normalize(&stop.stop_name),
            });
        }
        search_index.sort_by(|a, b| a.name_lower.cmp(&b.name_lower));
        search_index.dedup_by(|a, b| a.name_lower == b.name_lower);
        info!("{} searchable stops", search_index.len());

        let stats = GtfsStats {
            agencies: raw_agencies,
            routes: raw_routes,
            stops: raw_stops,
            trips: raw_trips,
            stop_times: raw_stop_times,
            calendars: raw_calendars,
            calendar_dates: raw_calendar_dates,
            transfers: raw_transfers,
            patterns: patterns.len(),
            services: service_ids.len(),
            loaded_at: chrono::Utc::now(),
        };

        RaptorData {
            stop_index,
            patterns,
            stop_patterns,
            stop_transfers,
            calendars: gtfs.calendars,
            calendar_exceptions,
            routes: gtfs.routes,
            stops,
            service_ids,
            search_index,
            stats,
        }
    }

    // -----------------------------------------------------------------------
    // Service calendar
    // -----------------------------------------------------------------------

    /// Pre-compute which services are active on a given date.
    /// Returns a `Vec<bool>` indexed by `service_idx` for O(1) lookup during queries.
    pub fn active_services(&self, date: &str) -> Vec<bool> {
        self.service_ids
            .iter()
            .map(|sid| self.is_service_active(sid, date))
            .collect()
    }

    /// Check if a service runs on a given date (YYYYMMDD).
    /// Calendar exceptions (added/removed) take priority over the regular calendar.
    fn is_service_active(&self, service_id: &str, date: &str) -> bool {
        if let Some(exceptions) = self.calendar_exceptions.get(service_id) {
            for (exc_date, exc_type) in exceptions {
                if exc_date == date {
                    return *exc_type == 1; // 1 = added, 2 = removed
                }
            }
        }

        if let Some(cal) = self.calendars.get(service_id) {
            if date < cal.start_date.as_str() || date > cal.end_date.as_str() {
                return false;
            }
            let weekday = date_to_weekday(date);
            return match weekday {
                0 => cal.monday == 1,
                1 => cal.tuesday == 1,
                2 => cal.wednesday == 1,
                3 => cal.thursday == 1,
                4 => cal.friday == 1,
                5 => cal.saturday == 1,
                6 => cal.sunday == 1,
                _ => false,
            };
        }

        false
    }

    // -----------------------------------------------------------------------
    // Stop search & resolution
    // -----------------------------------------------------------------------

    /// Search stops by name for autocomplete.
    ///
    /// Returns up to `limit` results as `(stop_idx, stop_name, stop_id)` tuples,
    /// ranked by relevance: exact match > prefix > word-prefix > substring.
    pub fn search_stops(&self, query: &str, limit: usize) -> Vec<(usize, &str, &str)> {
        if query.is_empty() {
            return Vec::new();
        }

        let q = normalize(query);
        let mut results: Vec<(usize, &str, &str, usize)> = Vec::new();

        for entry in &self.search_index {
            let rank = if entry.name_lower == q {
                0 // exact match
            } else if entry.name_lower.starts_with(&q) {
                1 // prefix
            } else if entry
                .name_lower
                .split_whitespace()
                .any(|w| w.starts_with(&q))
            {
                2 // word starts with
            } else if entry.name_lower.contains(&q) {
                3 // substring
            } else {
                continue;
            };

            let stop = &self.stops[entry.stop_idx];
            results.push((entry.stop_idx, &stop.stop_name, &stop.stop_id, rank));

            if results.len() >= limit * 10 {
                break;
            }
        }

        // Sort by relevance, then by name length (shorter = more specific)
        results.sort_by_key(|r| (r.3, r.1.len()));
        results.truncate(limit);
        results
            .into_iter()
            .map(|(idx, name, id, _)| (idx, name, id))
            .collect()
    }

    /// Resolve a user-provided stop identifier to a numeric stop index.
    ///
    /// Accepts either a direct stop_id (e.g. "IDFM:22101") or GPS coordinates
    /// in "lon;lat" format (e.g. "2.3522;48.8566"), in which case the nearest
    /// stop is returned.
    pub fn resolve_stop(&self, input: &str) -> Option<usize> {
        if let Some(&idx) = self.stop_index.get(input) {
            return Some(idx);
        }

        if let Some((lon_str, lat_str)) = input.split_once(';')
            && let (Ok(lon), Ok(lat)) = (lon_str.parse::<f64>(), lat_str.parse::<f64>())
        {
            return self.find_nearest_stop(lon, lat);
        }

        None
    }

    /// Brute-force nearest-stop search. Acceptable for single-query use.
    fn find_nearest_stop(&self, lon: f64, lat: f64) -> Option<usize> {
        self.stops
            .iter()
            .enumerate()
            .filter(|(_, s)| s.stop_lon != 0.0 || s.stop_lat != 0.0)
            .min_by(|(_, a), (_, b)| {
                let dist_a = haversine_approx(lat, lon, a.stop_lat, a.stop_lon);
                let dist_b = haversine_approx(lat, lon, b.stop_lat, b.stop_lon);
                dist_a
                    .partial_cmp(&dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
    }
}

// ---------------------------------------------------------------------------
// Text normalization
// ---------------------------------------------------------------------------

/// Normalize a string for fuzzy search: lowercase, strip French diacritics,
/// replace hyphens and apostrophes with spaces.
#[allow(clippy::collapsible_str_replace)]
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .replace('é', "e")
        .replace('è', "e")
        .replace('ê', "e")
        .replace('ë', "e")
        .replace('à', "a")
        .replace('â', "a")
        .replace('ä', "a")
        .replace('ô', "o")
        .replace('ö', "o")
        .replace('ù', "u")
        .replace('û', "u")
        .replace('ü', "u")
        .replace('î', "i")
        .replace('ï', "i")
        .replace('ç', "c")
        .replace('œ', "oe")
        .replace('æ', "ae")
        .replace(['-', '\''], " ")
        .replace('\u{2019}', " ")
        .replace('\u{2018}', " ")
}

/// Approximate squared distance between two points (sufficient for ranking).
fn haversine_approx(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = lat2 - lat1;
    let dlon = (lon2 - lon1) * lat1.to_radians().cos();
    dlat * dlat + dlon * dlon
}

// ---------------------------------------------------------------------------
// RAPTOR query
// ---------------------------------------------------------------------------

/// Label used during journey reconstruction to trace how each stop was reached.
#[derive(Debug, Clone)]
enum Label {
    /// Reached by boarding a vehicle trip.
    Trip {
        pattern_idx: usize,
        trip_idx: usize,
        board_pos: usize,
        alight_pos: usize,
    },
    /// Reached by walking from another stop.
    Transfer { from_stop: usize, duration: u32 },
}

/// Internal result of a RAPTOR query, containing arrival times and labels
/// for all stops across all rounds.
pub struct RaptorResult {
    /// `tau[k][stop]` = best arrival time at `stop` using at most `k` vehicle trips.
    tau: Vec<Vec<u32>>,
    /// `labels[k][stop]` = how we reached `stop` in round `k`.
    labels: Vec<Vec<Option<Label>>>,
    /// Source stop index (for reconstruction termination).
    source: usize,
}

/// Run the RAPTOR algorithm to find earliest-arrival journeys.
///
/// # Arguments
/// - `data`: pre-built RAPTOR data
/// - `source`, `target`: numeric stop indices
/// - `departure_time`: seconds since midnight
/// - `date`: date in YYYYMMDD format (for service calendar filtering)
/// - `max_transfers`: maximum number of vehicle changes allowed
/// - `excluded_patterns`: patterns to skip (used for diverse route alternatives)
pub fn raptor_query(
    data: &RaptorData,
    source: usize,
    target: usize,
    departure_time: u32,
    date: &str,
    max_transfers: usize,
    excluded_patterns: &HashSet<usize>,
) -> RaptorResult {
    let n = data.stops.len();
    let rounds = max_transfers.min(MAX_ROUNDS - 1) + 1;

    // Pre-compute active services for this date (avoids per-trip string lookups)
    let active = data.active_services(date);

    let mut tau: Vec<Vec<u32>> = vec![vec![INFINITY; n]; rounds + 1];
    let mut best: Vec<u32> = vec![INFINITY; n];
    let mut labels: Vec<Vec<Option<Label>>> = vec![vec![None; n]; rounds + 1];

    // Initialize source stop
    tau[0][source] = departure_time;
    best[source] = departure_time;

    // Apply initial foot transfers from source
    for &(to_stop, duration) in &data.stop_transfers[source] {
        let arr = departure_time.saturating_add(duration);
        if arr < tau[0][to_stop] {
            tau[0][to_stop] = arr;
            best[to_stop] = arr;
            labels[0][to_stop] = Some(Label::Transfer {
                from_stop: source,
                duration,
            });
        }
    }

    // Track which stops were improved (to limit route scanning)
    let mut marked: Vec<bool> = vec![false; n];
    marked[source] = true;
    for &(to_stop, _) in &data.stop_transfers[source] {
        if tau[0][to_stop] != INFINITY {
            marked[to_stop] = true;
        }
    }

    // Main RAPTOR loop: one round per additional vehicle trip
    for k in 1..=rounds {
        // Collect route patterns serving any marked stop
        let mut routes_to_scan: HashMap<usize, usize> = HashMap::new();
        for (stop_idx, is_marked) in marked.iter().enumerate() {
            if !*is_marked {
                continue;
            }
            for &(pat_idx, pos) in &data.stop_patterns[stop_idx] {
                if excluded_patterns.contains(&pat_idx) {
                    continue;
                }
                routes_to_scan
                    .entry(pat_idx)
                    .and_modify(|earliest| {
                        if pos < *earliest {
                            *earliest = pos;
                        }
                    })
                    .or_insert(pos);
            }
        }

        let mut new_marked: Vec<bool> = vec![false; n];

        // Scan each collected route pattern
        for (&pat_idx, &start_pos) in &routes_to_scan {
            let pattern = &data.patterns[pat_idx];
            let mut current_trip: Option<usize> = None;
            let mut board_pos: usize = 0;

            for pos in start_pos..pattern.stops.len() {
                let stop_idx = pattern.stops[pos];

                // Try to board an earlier trip at this stop
                if tau[k - 1][stop_idx] != INFINITY {
                    let board_time = tau[k - 1][stop_idx];
                    let new_trip = find_earliest_trip(pattern, pos, board_time, &active);

                    if let Some(trip_idx) = new_trip {
                        match current_trip {
                            None => {
                                current_trip = Some(trip_idx);
                                board_pos = pos;
                            }
                            Some(curr) => {
                                // Switch to this trip if it arrives earlier
                                let curr_arr = pattern.trips[curr].stop_times[pos].0;
                                let new_arr = pattern.trips[trip_idx].stop_times[pos].0;
                                if new_arr < curr_arr {
                                    current_trip = Some(trip_idx);
                                    board_pos = pos;
                                }
                            }
                        }
                    }
                }

                // If on a trip, update arrival time at this stop
                if let Some(trip_idx) = current_trip {
                    let arr = pattern.trips[trip_idx].stop_times[pos].0;
                    if arr < best[stop_idx] {
                        tau[k][stop_idx] = arr;
                        best[stop_idx] = arr;
                        labels[k][stop_idx] = Some(Label::Trip {
                            pattern_idx: pat_idx,
                            trip_idx,
                            board_pos,
                            alight_pos: pos,
                        });
                        new_marked[stop_idx] = true;
                    }
                }
            }
        }

        // Apply transfers ONLY from stops improved by trips (not by other transfers)
        // to prevent transitive transfer chains within a single round.
        let trip_improved: Vec<usize> = (0..n)
            .filter(|&s| matches!(labels[k][s], Some(Label::Trip { .. })))
            .collect();

        for &stop_idx in &trip_improved {
            for &(to_stop, duration) in &data.stop_transfers[stop_idx] {
                let arr = tau[k][stop_idx].saturating_add(duration);
                if arr < best[to_stop] {
                    tau[k][to_stop] = arr;
                    best[to_stop] = arr;
                    labels[k][to_stop] = Some(Label::Transfer {
                        from_stop: stop_idx,
                        duration,
                    });
                    new_marked[to_stop] = true;
                }
            }
        }

        marked = new_marked;

        // Early termination if no stop was improved
        if !marked.iter().any(|&m| m) {
            break;
        }

        // Early exit if target is reached and no marked stop can beat it
        if best[target] != INFINITY && !marked[target] {
            let dominated = !marked
                .iter()
                .enumerate()
                .any(|(s, &m)| m && best[s] < best[target]);
            if dominated {
                break;
            }
        }
    }

    RaptorResult {
        tau,
        labels,
        source,
    }
}

/// Find the earliest active trip departing at or after `min_departure`
/// at position `pos` within a pattern.
///
/// Uses linear scan for correctness: trips are sorted by departure at the
/// first stop, but not necessarily at intermediate stops.
fn find_earliest_trip(
    pattern: &Pattern,
    pos: usize,
    min_departure: u32,
    active: &[bool],
) -> Option<usize> {
    let mut best_dep = INFINITY;
    let mut best_idx = None;

    for (idx, trip) in pattern.trips.iter().enumerate() {
        let dep = trip.stop_times[pos].1;
        if dep >= min_departure && dep < best_dep && active[trip.service_idx] {
            best_dep = dep;
            best_idx = Some(idx);
        }
    }

    best_idx
}

// ---------------------------------------------------------------------------
// Journey reconstruction
// ---------------------------------------------------------------------------

/// A single section of a reconstructed journey (either a PT leg or a transfer).
pub struct JourneySection {
    pub section_type: SectionType,
    pub from_stop: usize,
    pub to_stop: usize,
    pub departure_time: u32,
    pub arrival_time: u32,
    /// Pattern index (only for PT sections).
    pub pattern_idx: Option<usize>,
    /// Trip index within the pattern (only for PT sections).
    pub trip_idx: Option<usize>,
    /// Boarding position within the pattern (only for PT sections).
    pub board_pos: Option<usize>,
    /// Alighting position within the pattern (only for PT sections).
    pub alight_pos: Option<usize>,
}

/// Collect the set of pattern indices used in a journey (for diversity filtering).
pub fn used_patterns(sections: &[JourneySection]) -> HashSet<usize> {
    sections.iter().filter_map(|s| s.pattern_idx).collect()
}

pub enum SectionType {
    PublicTransport,
    Transfer,
}

/// Reconstruct Pareto-optimal journeys from a RAPTOR result.
///
/// Each RAPTOR round that improves the arrival at the target yields a
/// distinct journey. This produces a trade-off set: more transfers but
/// earlier arrival vs. fewer transfers but later arrival.
pub fn reconstruct_journeys(
    data: &RaptorData,
    result: &RaptorResult,
    target: usize,
) -> Vec<Vec<JourneySection>> {
    let mut journeys = Vec::new();
    let mut best_time = INFINITY;

    for k in 0..result.tau.len() {
        let time = result.tau[k][target];
        if time < best_time {
            best_time = time;
            if let Some(sections) = reconstruct_for_round(data, result, target, k)
                && !sections.is_empty()
            {
                let clean = sanitize_sections(sections);
                if !clean.is_empty() {
                    journeys.push(clean);
                }
            }
        }
    }

    // Deduplicate journeys with identical arrival times
    journeys.dedup_by(|a, b| {
        let arr_a = a.last().map(|s| s.arrival_time).unwrap_or(0);
        let arr_b = b.last().map(|s| s.arrival_time).unwrap_or(0);
        arr_a == arr_b
    });

    journeys
}

/// Remove degenerate sections and merge consecutive transfers.
///
/// Filters out:
/// - PT sections where from == to (boarding/alighting artifact)
/// - PT sections with zero or negative duration
/// - Self-loop transfers with zero duration
///
/// Merges consecutive transfer sections into a single walk.
fn sanitize_sections(sections: Vec<JourneySection>) -> Vec<JourneySection> {
    let mut result: Vec<JourneySection> = Vec::new();

    for section in sections {
        match section.section_type {
            SectionType::PublicTransport => {
                if section.from_stop == section.to_stop {
                    continue;
                }
                if section.arrival_time <= section.departure_time {
                    continue;
                }
                result.push(section);
            }
            SectionType::Transfer => {
                if section.from_stop == section.to_stop
                    && section.arrival_time == section.departure_time
                {
                    continue;
                }
                if let Some(last) = result.last_mut()
                    && matches!(last.section_type, SectionType::Transfer)
                {
                    last.to_stop = section.to_stop;
                    last.arrival_time = section.arrival_time;
                    continue;
                }
                result.push(section);
            }
        }
    }

    result
}

/// Reconstruct the journey for a specific RAPTOR round by tracing labels
/// backwards from the target to the source.
fn reconstruct_for_round(
    data: &RaptorData,
    result: &RaptorResult,
    target: usize,
    round: usize,
) -> Option<Vec<JourneySection>> {
    if result.tau[round][target] == INFINITY {
        return None;
    }

    let mut sections: Vec<JourneySection> = Vec::new();
    let mut current_stop = target;
    let mut current_round = round;

    loop {
        if current_stop == result.source {
            break;
        }

        match &result.labels[current_round][current_stop] {
            Some(Label::Trip {
                pattern_idx,
                trip_idx,
                board_pos,
                alight_pos,
            }) => {
                let pattern = &data.patterns[*pattern_idx];
                let trip = &pattern.trips[*trip_idx];

                sections.push(JourneySection {
                    section_type: SectionType::PublicTransport,
                    from_stop: pattern.stops[*board_pos],
                    to_stop: pattern.stops[*alight_pos],
                    departure_time: trip.stop_times[*board_pos].1,
                    arrival_time: trip.stop_times[*alight_pos].0,
                    pattern_idx: Some(*pattern_idx),
                    trip_idx: Some(*trip_idx),
                    board_pos: Some(*board_pos),
                    alight_pos: Some(*alight_pos),
                });

                current_stop = pattern.stops[*board_pos];
                if current_round == 0 {
                    break;
                }
                current_round -= 1;
            }
            Some(Label::Transfer {
                from_stop,
                duration,
            }) => {
                let arr = result.tau[current_round][current_stop];
                sections.push(JourneySection {
                    section_type: SectionType::Transfer,
                    from_stop: *from_stop,
                    to_stop: current_stop,
                    departure_time: arr.saturating_sub(*duration),
                    arrival_time: arr,
                    pattern_idx: None,
                    trip_idx: None,
                    board_pos: None,
                    alight_pos: None,
                });
                current_stop = *from_stop;
            }
            None => break,
        }
    }

    sections.reverse();
    Some(sections)
}

// ---------------------------------------------------------------------------
// Date & time utilities
// ---------------------------------------------------------------------------

/// Convert a YYYYMMDD date string to day of week (0=Monday, 6=Sunday).
/// Uses Tomohiko Sakamoto's algorithm.
fn date_to_weekday(date: &str) -> u32 {
    if date.len() != 8 {
        return 7; // invalid
    }
    let y: i32 = date[0..4].parse().unwrap_or(0);
    let m: i32 = date[4..6].parse().unwrap_or(0);
    let d: i32 = date[6..8].parse().unwrap_or(0);

    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let adj_y = if m < 3 { y - 1 } else { y };
    let w = (adj_y + adj_y / 4 - adj_y / 100 + adj_y / 400 + t[(m - 1) as usize] + d) % 7;
    ((w + 6) % 7) as u32 // Sakamoto: 0=Sunday → convert to 0=Monday
}

/// Format seconds since midnight as "YYYYMMDDTHHMMSS".
///
/// Handles GTFS times > 24h (e.g. 25:30:00) by rolling over to the next day
/// using chrono date arithmetic.
pub fn format_datetime(date: &str, seconds: u32) -> String {
    let total_h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;

    if total_h < 24 {
        return format!("{date}T{total_h:02}{m:02}{s:02}");
    }

    let extra_days = (total_h / 24) as i64;
    let h = total_h % 24;

    let y: i32 = date.get(0..4).and_then(|s| s.parse().ok()).unwrap_or(2026);
    let mo: u32 = date.get(4..6).and_then(|s| s.parse().ok()).unwrap_or(1);
    let d: u32 = date.get(6..8).and_then(|s| s.parse().ok()).unwrap_or(1);

    if let Some(base) = chrono::NaiveDate::from_ymd_opt(y, mo, d) {
        let rolled = base + chrono::Duration::days(extra_days);
        format!("{}T{h:02}{m:02}{s:02}", rolled.format("%Y%m%d"))
    } else {
        format!("{date}T{h:02}{m:02}{s:02}")
    }
}

/// Parse a datetime string into (date_str, seconds_since_midnight).
///
/// Accepted formats:
/// - `YYYYMMDDTHHmmss` (standard, with T separator)
/// - `YYYYMMDDTHHmm` (without seconds)
/// - `YYYYMMDDHHmmss` (without T separator)
/// - `YYYYMMDD` (date only, defaults to 08:00)
pub fn parse_datetime(input: &str) -> Option<(String, u32)> {
    let date = input.get(0..8)?;

    // Skip optional 'T' separator
    let time_start = if input.as_bytes().get(8) == Some(&b'T') {
        9
    } else {
        8
    };
    let time_part = input.get(time_start..)?;

    if time_part.is_empty() {
        return Some((date.to_string(), 8 * 3600));
    }

    let h: u32 = time_part.get(0..2)?.parse().ok()?;
    let m: u32 = time_part.get(2..4)?.parse().ok()?;
    let s: u32 = time_part
        .get(4..6)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    Some((date.to_string(), h * 3600 + m * 60 + s))
}
