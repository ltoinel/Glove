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

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
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
///
/// Field order is optimized for cache locality: hot fields used in the
/// RAPTOR inner loop (`service_idx`, `stop_times`) come first; cold fields
/// used only during reconstruction (`trip_id`, `headsign`) come last.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternTrip {
    /// Index into [`RaptorData::service_ids`] for fast active-service checks.
    pub service_idx: usize,
    /// (arrival, departure) in seconds since midnight, one per stop in the pattern.
    pub stop_times: Vec<(u32, u32)>,
    #[allow(dead_code)]
    pub trip_id: String,
    /// Destination sign displayed on the vehicle.
    pub headsign: String,
}

/// A route pattern: an ordered sequence of stops served by multiple trips.
///
/// Trips within the same pattern visit the exact same stops in the same order.
/// They are sorted by departure time at the first stop.
#[derive(Debug, Serialize, Deserialize)]
pub struct Pattern {
    pub route_id: String,
    /// Ordered numeric stop indices for this pattern.
    pub stops: Vec<usize>,
    /// Trips sorted by departure at the first stop.
    pub trips: Vec<PatternTrip>,
}

/// Aggregate statistics captured during GTFS loading and RAPTOR index build.
#[derive(Serialize, Deserialize)]
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
#[derive(Serialize, Deserialize)]
pub struct SearchEntry {
    pub stop_idx: usize,
    /// Lowercased, diacritics-stripped name for fuzzy matching.
    pub name_lower: String,
}

/// The complete RAPTOR data structure, built once from GTFS and shared
/// across all request handlers via [`ArcSwap`](arc_swap::ArcSwap).
#[derive(Serialize, Deserialize)]
pub struct RaptorData {
    /// stop_id (String) → numeric index (FxHashMap for faster lookups).
    pub stop_index: FxHashMap<String, usize>,
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
    /// Calendar exceptions: `service_id → { date → exception_type }` for O(1) lookup.
    calendar_exceptions: FxHashMap<String, FxHashMap<String, u8>>,
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
        let mut stop_index: FxHashMap<String, usize> = FxHashMap::default();
        let mut stops: Vec<gtfs::Stop> = Vec::with_capacity(gtfs.stops.len());

        for (id, stop) in &gtfs.stops {
            let idx = stops.len();
            stop_index.insert(id.clone(), idx);
            stops.push(stop.clone());
        }

        let num_stops = stops.len();
        info!("{} stops indexed", num_stops);

        // Step 2: Intern service_ids for O(1) active-service checks
        let mut service_index: FxHashMap<String, usize> = FxHashMap::default();
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
        let mut pattern_map: FxHashMap<Vec<usize>, usize> = FxHashMap::default();
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
                service_idx: svc_idx,
                stop_times: stop_times_parsed,
                trip_id: trip_id.to_string(),
                headsign: trip.trip_headsign.clone(),
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

        // Build pathway graph: stop_id → Vec<(stop_id, traversal_time)>
        // Pathways provide realistic walking times within stations
        info!(
            "Building pathway graph ({} pathways)...",
            gtfs.pathways.len()
        );
        let mut pathway_graph: FxHashMap<&str, Vec<(&str, u32)>> = FxHashMap::default();
        for p in &gtfs.pathways {
            if let Some(time) = p.traversal_time {
                pathway_graph
                    .entry(&p.from_stop_id)
                    .or_default()
                    .push((&p.to_stop_id, time));
                if p.is_bidirectional == 1 {
                    pathway_graph
                        .entry(&p.to_stop_id)
                        .or_default()
                        .push((&p.from_stop_id, time));
                }
            }
        }

        // Add implicit transfers between stops sharing the same parent station
        // Use pathway traversal times when available, otherwise default_transfer_time
        info!("Building parent station transfers...");
        let mut parent_stops: FxHashMap<&str, Vec<usize>> = FxHashMap::default();
        for (idx, stop) in stops.iter().enumerate() {
            if !stop.parent_station.is_empty() {
                parent_stops
                    .entry(&stop.parent_station)
                    .or_default()
                    .push(idx);
            }
        }

        // Compute shortest pathway times between stops via BFS on the pathway graph.
        // For two sibling stops A and B in the same station, finds the shortest
        // path through intermediate nodes (entrances, platforms) using Dijkstra.
        let pathway_time = |from_id: &str, to_id: &str| -> Option<u32> {
            if pathway_graph.is_empty() {
                return None;
            }
            // Mini-Dijkstra on the pathway graph
            let mut dist: FxHashMap<&str, u32> = FxHashMap::default();
            let mut queue: std::collections::BinaryHeap<std::cmp::Reverse<(u32, &str)>> =
                std::collections::BinaryHeap::new();
            dist.insert(from_id, 0);
            queue.push(std::cmp::Reverse((0, from_id)));
            while let Some(std::cmp::Reverse((cost, node))) = queue.pop() {
                if node == to_id {
                    return Some(cost);
                }
                if cost > dist.get(node).copied().unwrap_or(u32::MAX) {
                    continue;
                }
                if let Some(neighbors) = pathway_graph.get(node) {
                    for &(next, time) in neighbors {
                        let new_cost = cost + time;
                        if new_cost < dist.get(next).copied().unwrap_or(u32::MAX) {
                            dist.insert(next, new_cost);
                            queue.push(std::cmp::Reverse((new_cost, next)));
                        }
                    }
                }
            }
            None
        };

        let mut parent_transfer_count = 0u64;
        let mut pathway_transfer_count = 0u64;
        for siblings in parent_stops.values() {
            if siblings.len() < 2 {
                continue;
            }
            for &a in siblings {
                let existing: FxHashSet<usize> =
                    stop_transfers[a].iter().map(|&(t, _)| t).collect();
                for &b in siblings {
                    if a != b && !existing.contains(&b) {
                        let duration = pathway_time(&stops[a].stop_id, &stops[b].stop_id)
                            .unwrap_or(default_transfer_time);
                        if duration != default_transfer_time {
                            pathway_transfer_count += 1;
                        }
                        stop_transfers[a].push((b, duration));
                        parent_transfer_count += 1;
                    }
                }
            }
        }
        info!(
            "{} parent station transfers added ({} with pathway times)",
            parent_transfer_count, pathway_transfer_count
        );

        // Calendar exceptions index: service_id → { date → exception_type } for O(1) lookup
        let mut calendar_exceptions: FxHashMap<String, FxHashMap<String, u8>> =
            FxHashMap::default();
        for cd in &gtfs.calendar_dates {
            calendar_exceptions
                .entry(cd.service_id.clone())
                .or_default()
                .insert(cd.date.clone(), cd.exception_type);
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
    // Cache persistence
    // -----------------------------------------------------------------------

    /// Save the RAPTOR index to a binary cache file.
    pub fn save(
        &self,
        cache_dir: &Path,
        fingerprint: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(cache_dir)?;
        let bin_path = cache_dir.join("raptor.bin");
        let fp_path = cache_dir.join("raptor.fingerprint");

        let encoded = bincode::serialize(self)?;
        std::fs::write(&bin_path, &encoded)?;
        std::fs::write(&fp_path, fingerprint)?;

        info!(
            "RAPTOR index saved to {} ({:.1} MB)",
            bin_path.display(),
            encoded.len() as f64 / 1_048_576.0
        );
        Ok(())
    }

    /// Load the RAPTOR index from cache if the fingerprint matches.
    ///
    /// Returns `None` if the cache does not exist or the fingerprint is stale.
    pub fn load_cached(cache_dir: &Path, fingerprint: &str) -> Option<Self> {
        let bin_path = cache_dir.join("raptor.bin");
        let fp_path = cache_dir.join("raptor.fingerprint");

        let cached_fp = std::fs::read_to_string(&fp_path).ok()?;
        if cached_fp.trim() != fingerprint {
            info!("RAPTOR cache fingerprint mismatch, rebuilding");
            return None;
        }

        let bytes = std::fs::read(&bin_path).ok()?;
        match bincode::deserialize(&bytes) {
            Ok(data) => {
                info!("RAPTOR index loaded from cache ({})", bin_path.display());
                Some(data)
            }
            Err(e) => {
                info!("RAPTOR cache corrupted, rebuilding: {e}");
                None
            }
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
        if let Some(exceptions) = self.calendar_exceptions.get(service_id)
            && let Some(&exc_type) = exceptions.get(date)
        {
            return exc_type == 1; // 1 = added, 2 = removed
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
    /// served stop within `max_distance_m` meters is returned.
    pub fn resolve_stop(&self, input: &str, max_distance_m: u32) -> Option<usize> {
        if let Some(&idx) = self.stop_index.get(input) {
            return Some(idx);
        }

        if let Some((lon_str, lat_str)) = input.split_once(';')
            && let (Ok(lon), Ok(lat)) = (lon_str.parse::<f64>(), lat_str.parse::<f64>())
        {
            return self.find_nearest_stop(lon, lat, max_distance_m);
        }

        None
    }

    /// Find the nearest stop that is served by at least one pattern
    /// (i.e. reachable by RAPTOR) and within `max_distance_m` meters.
    /// Stops without patterns (entrances, parent stations, unused quays) are ignored.
    fn find_nearest_stop(&self, lon: f64, lat: f64, max_distance_m: u32) -> Option<usize> {
        // Pre-filter: rough bounding box in degrees (~1 degree ≈ 111 km)
        let max_deg = max_distance_m as f64 / 111_000.0 * 1.5; // 1.5x margin

        self.stops
            .iter()
            .enumerate()
            .filter(|(idx, s)| {
                (s.stop_lon != 0.0 || s.stop_lat != 0.0)
                    && !self.stop_patterns[*idx].is_empty()
                    && (s.stop_lat - lat).abs() < max_deg
                    && (s.stop_lon - lon).abs() < max_deg
            })
            .min_by(|(_, a), (_, b)| {
                let dist_a = haversine_approx(lat, lon, a.stop_lat, a.stop_lon);
                let dist_b = haversine_approx(lat, lon, b.stop_lat, b.stop_lon);
                dist_a
                    .partial_cmp(&dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .filter(|(_, s)| {
                haversine_meters(lat, lon, s.stop_lat, s.stop_lon) <= max_distance_m as f64
            })
            .map(|(idx, _)| idx)
    }

    /// Find all served stops within `max_distance_m` meters of the given
    /// coordinates. Returns `(stop_idx, walking_time_secs)` pairs, where
    /// walking time is computed from the straight-line distance at the given
    /// `walking_speed` (km/h, defaults to 5).
    pub fn find_nearby_stops(
        &self,
        lon: f64,
        lat: f64,
        max_distance_m: u32,
        walking_speed_kmh: f64,
    ) -> Vec<(usize, u32)> {
        let max_deg = max_distance_m as f64 / 111_000.0 * 1.5;
        let speed_ms = walking_speed_kmh / 3.6; // m/s

        self.stops
            .iter()
            .enumerate()
            .filter(|(idx, s)| {
                (s.stop_lon != 0.0 || s.stop_lat != 0.0)
                    && !self.stop_patterns[*idx].is_empty()
                    && (s.stop_lat - lat).abs() < max_deg
                    && (s.stop_lon - lon).abs() < max_deg
            })
            .filter_map(|(idx, s)| {
                let dist = haversine_meters(lat, lon, s.stop_lat, s.stop_lon);
                if dist <= max_distance_m as f64 {
                    let walk_secs = (dist / speed_ms).ceil() as u32;
                    Some((idx, walk_secs))
                } else {
                    None
                }
            })
            .collect()
    }
}

use crate::text::normalize;

/// Approximate squared distance between two points (sufficient for ranking).
fn haversine_approx(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = lat2 - lat1;
    let dlon = (lon2 - lon1) * lat1.to_radians().cos();
    dlat * dlat + dlon * dlon
}

/// Haversine distance in meters between two points.
fn haversine_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0; // Earth radius in meters
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
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
    pub tau: Vec<Vec<u32>>,
    /// `labels[k][stop]` = how we reached `stop` in round `k`.
    labels: Vec<Vec<Option<Label>>>,
    /// Source stop indices (for reconstruction termination).
    sources: FxHashSet<usize>,
}

/// Run the RAPTOR algorithm to find earliest-arrival journeys.
///
/// # Arguments
/// - `data`: pre-built RAPTOR data
/// - `source`, `target`: numeric stop indices
/// - `departure_time`: seconds since midnight
/// - `active`: pre-computed active services bitmap (from [`RaptorData::active_services`])
/// - `max_transfers`: maximum number of vehicle changes allowed
/// - `excluded_patterns`: patterns to skip (used for diverse route alternatives)
pub fn raptor_query(
    data: &RaptorData,
    sources: &[(usize, u32)],
    departure_time: u32,
    active: &[bool],
    max_transfers: usize,
    excluded_patterns: &FxHashSet<usize>,
) -> RaptorResult {
    let n = data.stops.len();
    let rounds = max_transfers.min(MAX_ROUNDS - 1) + 1;

    let mut tau: Vec<Vec<u32>> = vec![vec![INFINITY; n]; rounds + 1];
    let mut best: Vec<u32> = vec![INFINITY; n];
    let mut labels: Vec<Vec<Option<Label>>> = vec![vec![None; n]; rounds + 1];

    // Track which stops were improved (to limit route scanning)
    let mut marked: Vec<bool> = vec![false; n];

    // Initialize all source stops with their walking-time offset
    for &(source, walk_time) in sources {
        let arr = departure_time.saturating_add(walk_time);
        if arr < tau[0][source] {
            tau[0][source] = arr;
            best[source] = arr;
            marked[source] = true;
        }

        // Apply initial foot transfers from each source
        for &(to_stop, duration) in &data.stop_transfers[source] {
            let total = arr.saturating_add(duration);
            if total < tau[0][to_stop] {
                tau[0][to_stop] = total;
                best[to_stop] = total;
                labels[0][to_stop] = Some(Label::Transfer {
                    from_stop: source,
                    duration,
                });
                marked[to_stop] = true;
            }
        }
    }

    // Pre-allocated buffers reused across rounds
    let num_patterns = data.patterns.len();
    let mut route_earliest: Vec<usize> = vec![usize::MAX; num_patterns]; // earliest position per pattern
    let mut active_routes: Vec<usize> = Vec::with_capacity(num_patterns); // patterns to scan this round
    let mut new_marked: Vec<bool> = vec![false; n];
    let mut trip_improved: Vec<usize> = Vec::new(); // stops improved by trips (for transfers)

    // Main RAPTOR loop: one round per additional vehicle trip
    for k in 1..=rounds {
        // Collect route patterns serving any marked stop (Vec-based, no HashMap)
        active_routes.clear();
        for (stop_idx, is_marked) in marked.iter().enumerate() {
            if !*is_marked {
                continue;
            }
            for &(pat_idx, pos) in &data.stop_patterns[stop_idx] {
                if excluded_patterns.contains(&pat_idx) {
                    continue;
                }
                if route_earliest[pat_idx] == usize::MAX {
                    active_routes.push(pat_idx);
                    route_earliest[pat_idx] = pos;
                } else if pos < route_earliest[pat_idx] {
                    route_earliest[pat_idx] = pos;
                }
            }
        }

        new_marked.fill(false);
        trip_improved.clear();

        // Scan each collected route pattern
        for &pat_idx in &active_routes {
            let start_pos = route_earliest[pat_idx];
            route_earliest[pat_idx] = usize::MAX; // reset for next round

            let pattern = &data.patterns[pat_idx];
            let mut current_trip: Option<usize> = None;
            let mut board_pos: usize = 0;

            for pos in start_pos..pattern.stops.len() {
                let stop_idx = pattern.stops[pos];

                // Try to board an earlier trip at this stop
                if tau[k - 1][stop_idx] != INFINITY {
                    let board_time = tau[k - 1][stop_idx];
                    let new_trip = find_earliest_trip(pattern, pos, board_time, active);

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
                        trip_improved.push(stop_idx); // collect inline
                    }
                }
            }
        }

        // Apply transfers ONLY from stops improved by trips (not by other transfers)
        // to prevent transitive transfer chains within a single round.
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

        std::mem::swap(&mut marked, &mut new_marked);

        // Early termination if no stop was improved
        if !marked.iter().any(|&m| m) {
            break;
        }
    }

    let source_set: FxHashSet<usize> = sources.iter().map(|&(idx, _)| idx).collect();
    RaptorResult {
        tau,
        labels,
        sources: source_set,
    }
}

/// Find the earliest active trip departing at or after `min_departure`
/// at position `pos` within a pattern.
///
/// Trips are sorted by departure at the first stop. We use binary search
/// on the first-stop departure to skip trips that depart too early, then
/// scan forward for the best match at `pos`.
fn find_earliest_trip(
    pattern: &Pattern,
    pos: usize,
    min_departure: u32,
    active: &[bool],
) -> Option<usize> {
    let trips = &pattern.trips;

    // Binary search: find first trip whose first-stop departure >= min_departure.
    // Trips departing before this at stop 0 *might* still be valid at `pos` due to
    // travel time, so we look back a small window for safety.
    let pivot = trips.partition_point(|t| t.stop_times[0].1 < min_departure);
    let start = pivot.saturating_sub(8); // look-back window for intermediate stops

    let mut best_dep = INFINITY;
    let mut best_idx = None;

    for (idx, trip) in trips.iter().enumerate().skip(start) {
        let dep = trip.stop_times[pos].1;
        if dep >= min_departure && dep < best_dep && active[trip.service_idx] {
            best_dep = dep;
            best_idx = Some(idx);
            // Once past the pivot, trips are roughly ordered: if we found a match
            // and the first-stop departure is already well past our best, stop early.
            if idx > pivot && trip.stop_times[0].1 > best_dep {
                break;
            }
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
pub fn used_patterns(sections: &[JourneySection]) -> FxHashSet<usize> {
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
///
/// `targets` is a slice of `(stop_idx, walking_time_secs)` — the walking
/// time from the stop to the final destination is added to determine the
/// effective arrival time.
pub fn reconstruct_journeys(
    data: &RaptorData,
    result: &RaptorResult,
    targets: &[(usize, u32)],
) -> Vec<Vec<JourneySection>> {
    let mut journeys = Vec::new();
    let mut best_time = INFINITY;

    for k in 0..result.tau.len() {
        // Pick the target stop with the best effective arrival (PT arrival + walk)
        let best_target = targets
            .iter()
            .filter(|&&(t, _)| result.tau[k][t] < INFINITY)
            .min_by_key(|&&(t, walk)| result.tau[k][t].saturating_add(walk));
        let Some(&(target, walk_to_dest)) = best_target else {
            continue;
        };
        let time = result.tau[k][target].saturating_add(walk_to_dest);
        if time < best_time {
            best_time = time;
            if let Some(sections) = reconstruct_for_round(data, result, target, k)
                && !sections.is_empty()
            {
                let clean = sanitize_sections(data, sections);
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
fn sanitize_sections(_data: &RaptorData, sections: Vec<JourneySection>) -> Vec<JourneySection> {
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
        if result.sources.contains(&current_stop) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs;

    // -----------------------------------------------------------------------
    // format_datetime
    // -----------------------------------------------------------------------

    #[test]
    fn format_datetime_normal() {
        assert_eq!(format_datetime("20260405", 0), "20260405T000000");
        assert_eq!(format_datetime("20260405", 30600), "20260405T083000");
        assert_eq!(format_datetime("20260405", 86399), "20260405T235959");
    }

    #[test]
    fn format_datetime_midnight() {
        assert_eq!(format_datetime("20260405", 86400), "20260406T000000");
    }

    #[test]
    fn format_datetime_past_midnight() {
        // 25h30 = 91800s → rolls over to next day 01:30
        assert_eq!(format_datetime("20260405", 91800), "20260406T013000");
    }

    #[test]
    fn format_datetime_month_rollover() {
        // Jan 31 + 24h = Feb 1
        assert_eq!(format_datetime("20260131", 86400), "20260201T000000");
    }

    // -----------------------------------------------------------------------
    // parse_datetime
    // -----------------------------------------------------------------------

    #[test]
    fn parse_datetime_with_t_and_seconds() {
        let (date, secs) = parse_datetime("20260405T083000").unwrap();
        assert_eq!(date, "20260405");
        assert_eq!(secs, 8 * 3600 + 30 * 60);
    }

    #[test]
    fn parse_datetime_with_t_no_seconds() {
        let (date, secs) = parse_datetime("20260405T0830").unwrap();
        assert_eq!(date, "20260405");
        assert_eq!(secs, 8 * 3600 + 30 * 60);
    }

    #[test]
    fn parse_datetime_no_t() {
        let (date, secs) = parse_datetime("20260405083000").unwrap();
        assert_eq!(date, "20260405");
        assert_eq!(secs, 8 * 3600 + 30 * 60);
    }

    #[test]
    fn parse_datetime_date_only() {
        let (date, secs) = parse_datetime("20260405").unwrap();
        assert_eq!(date, "20260405");
        assert_eq!(secs, 8 * 3600); // defaults to 08:00
    }

    #[test]
    fn parse_datetime_too_short() {
        assert!(parse_datetime("2026").is_none());
        assert!(parse_datetime("").is_none());
    }

    // -----------------------------------------------------------------------
    // date_to_weekday
    // -----------------------------------------------------------------------

    #[test]
    fn date_to_weekday_known_dates() {
        // 2026-04-05 is a Sunday
        assert_eq!(date_to_weekday("20260405"), 6);
        // 2026-04-06 is a Monday
        assert_eq!(date_to_weekday("20260406"), 0);
        // 2024-01-01 was a Monday
        assert_eq!(date_to_weekday("20240101"), 0);
        // 2024-02-29 was a Thursday (leap year)
        assert_eq!(date_to_weekday("20240229"), 3);
    }

    #[test]
    fn date_to_weekday_invalid() {
        assert_eq!(date_to_weekday("short"), 7);
        assert_eq!(date_to_weekday(""), 7);
    }

    // -----------------------------------------------------------------------
    // haversine_approx
    // -----------------------------------------------------------------------

    #[test]
    fn haversine_approx_same_point() {
        assert_eq!(haversine_approx(48.8566, 2.3522, 48.8566, 2.3522), 0.0);
    }

    #[test]
    fn haversine_approx_different_points() {
        let d = haversine_approx(48.8566, 2.3522, 48.8534, 2.3488);
        assert!(d > 0.0);
    }

    // -----------------------------------------------------------------------
    // Helper: build a minimal RaptorData for testing
    // -----------------------------------------------------------------------

    fn make_test_gtfs() -> gtfs::GtfsData {
        let mut stops = HashMap::new();
        stops.insert(
            "S1".to_string(),
            gtfs::Stop {
                stop_id: "S1".to_string(),
                stop_name: "Châtelet".to_string(),
                stop_lon: 2.347,
                stop_lat: 48.858,
                parent_station: "P1".to_string(),
            },
        );
        stops.insert(
            "S2".to_string(),
            gtfs::Stop {
                stop_id: "S2".to_string(),
                stop_name: "Gare de Lyon".to_string(),
                stop_lon: 2.373,
                stop_lat: 48.844,
                parent_station: String::new(),
            },
        );
        stops.insert(
            "S3".to_string(),
            gtfs::Stop {
                stop_id: "S3".to_string(),
                stop_name: "Nation".to_string(),
                stop_lon: 2.395,
                stop_lat: 48.848,
                parent_station: String::new(),
            },
        );
        stops.insert(
            "S4".to_string(),
            gtfs::Stop {
                stop_id: "S4".to_string(),
                stop_name: "Châtelet Quai 2".to_string(),
                stop_lon: 2.347,
                stop_lat: 48.858,
                parent_station: "P1".to_string(),
            },
        );

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
                trip_headsign: "Nation".to_string(),
            },
        );
        trips.insert(
            "T2".to_string(),
            gtfs::Trip {
                route_id: "R1".to_string(),
                service_id: "SVC1".to_string(),
                trip_id: "T2".to_string(),
                trip_headsign: "Nation".to_string(),
            },
        );

        let stop_times = vec![
            gtfs::StopTime {
                trip_id: "T1".to_string(),
                arrival_time: "08:00:00".to_string(),
                departure_time: "08:01:00".to_string(),
                stop_id: "S1".to_string(),
                stop_sequence: 0,
            },
            gtfs::StopTime {
                trip_id: "T1".to_string(),
                arrival_time: "08:10:00".to_string(),
                departure_time: "08:11:00".to_string(),
                stop_id: "S2".to_string(),
                stop_sequence: 1,
            },
            gtfs::StopTime {
                trip_id: "T1".to_string(),
                arrival_time: "08:20:00".to_string(),
                departure_time: "08:21:00".to_string(),
                stop_id: "S3".to_string(),
                stop_sequence: 2,
            },
            gtfs::StopTime {
                trip_id: "T2".to_string(),
                arrival_time: "09:00:00".to_string(),
                departure_time: "09:01:00".to_string(),
                stop_id: "S1".to_string(),
                stop_sequence: 0,
            },
            gtfs::StopTime {
                trip_id: "T2".to_string(),
                arrival_time: "09:10:00".to_string(),
                departure_time: "09:11:00".to_string(),
                stop_id: "S2".to_string(),
                stop_sequence: 1,
            },
            gtfs::StopTime {
                trip_id: "T2".to_string(),
                arrival_time: "09:20:00".to_string(),
                departure_time: "09:21:00".to_string(),
                stop_id: "S3".to_string(),
                stop_sequence: 2,
            },
        ];

        let mut calendars = HashMap::new();
        calendars.insert(
            "SVC1".to_string(),
            gtfs::Calendar {
                service_id: "SVC1".to_string(),
                monday: 1,
                tuesday: 1,
                wednesday: 1,
                thursday: 1,
                friday: 1,
                saturday: 1,
                sunday: 1,
                start_date: "20260101".to_string(),
                end_date: "20261231".to_string(),
            },
        );

        let calendar_dates = vec![gtfs::CalendarDate {
            service_id: "SVC1".to_string(),
            date: "20260101".to_string(),
            exception_type: 2, // removed on Jan 1
        }];

        let transfers = vec![gtfs::Transfer {
            from_stop_id: "S2".to_string(),
            to_stop_id: "S3".to_string(),
            min_transfer_time: Some(180),
        }];

        gtfs::GtfsData {
            agencies: vec![],
            routes,
            stops,
            trips,
            stop_times,
            calendars,
            calendar_dates,
            transfers,
            pathways: vec![],
        }
    }

    fn build_test_data() -> RaptorData {
        RaptorData::build(make_test_gtfs(), 120)
    }

    // -----------------------------------------------------------------------
    // RaptorData::build
    // -----------------------------------------------------------------------

    #[test]
    fn build_creates_patterns() {
        let data = build_test_data();
        assert!(!data.patterns.is_empty());
        // Both trips share the same stop sequence → 1 pattern with 2 trips
        assert_eq!(data.patterns.len(), 1);
        assert_eq!(data.patterns[0].trips.len(), 2);
    }

    #[test]
    fn build_indexes_stops() {
        let data = build_test_data();
        assert_eq!(data.stops.len(), 4);
        assert!(data.stop_index.contains_key("S1"));
        assert!(data.stop_index.contains_key("S2"));
        assert!(data.stop_index.contains_key("S3"));
    }

    #[test]
    fn build_creates_parent_station_transfers() {
        let data = build_test_data();
        // S1 and S4 share parent P1 → should have transfers between them
        let s1 = data.stop_index["S1"];
        let s4 = data.stop_index["S4"];
        let s1_targets: Vec<usize> = data.stop_transfers[s1].iter().map(|&(t, _)| t).collect();
        assert!(s1_targets.contains(&s4));
    }

    #[test]
    fn build_creates_explicit_transfers() {
        let data = build_test_data();
        let s2 = data.stop_index["S2"];
        let s3 = data.stop_index["S3"];
        let s2_targets: Vec<(usize, u32)> = data.stop_transfers[s2].clone();
        assert!(s2_targets.iter().any(|&(t, d)| t == s3 && d == 180));
    }

    #[test]
    fn build_sorts_trips_by_departure() {
        let data = build_test_data();
        let trips = &data.patterns[0].trips;
        assert!(trips[0].stop_times[0].1 <= trips[1].stop_times[0].1);
    }

    #[test]
    fn build_search_index() {
        let data = build_test_data();
        // S4 has no patterns → should not be in search index
        // S1, S2, S3 have patterns → should be searchable
        assert!(data.search_index.len() >= 3);
    }

    #[test]
    fn build_stats() {
        let data = build_test_data();
        assert_eq!(data.stats.stops, 4);
        assert_eq!(data.stats.trips, 2);
        assert_eq!(data.stats.patterns, 1);
    }

    // -----------------------------------------------------------------------
    // active_services
    // -----------------------------------------------------------------------

    #[test]
    fn active_services_normal_day() {
        let data = build_test_data();
        // 2026-04-06 is a Monday, SVC1 runs all days
        let active = data.active_services("20260406");
        let svc_idx = data.service_ids.iter().position(|s| s == "SVC1").unwrap();
        assert!(active[svc_idx]);
    }

    #[test]
    fn active_services_exception_removed() {
        let data = build_test_data();
        // SVC1 is removed on 20260101
        let active = data.active_services("20260101");
        let svc_idx = data.service_ids.iter().position(|s| s == "SVC1").unwrap();
        assert!(!active[svc_idx]);
    }

    #[test]
    fn active_services_outside_range() {
        let data = build_test_data();
        // 2027-01-01 is outside SVC1 end_date (20261231)
        let active = data.active_services("20270101");
        let svc_idx = data.service_ids.iter().position(|s| s == "SVC1").unwrap();
        assert!(!active[svc_idx]);
    }

    // -----------------------------------------------------------------------
    // search_stops
    // -----------------------------------------------------------------------

    #[test]
    fn search_stops_exact() {
        let data = build_test_data();
        let results = data.search_stops("Châtelet", 5);
        assert!(!results.is_empty());
        // Exact match should rank first (after normalization)
    }

    #[test]
    fn search_stops_prefix() {
        let data = build_test_data();
        let results = data.search_stops("Gare", 5);
        assert!(!results.is_empty());
        assert!(results.iter().any(|(_, name, _)| name.contains("Gare")));
    }

    #[test]
    fn search_stops_substring() {
        let data = build_test_data();
        let results = data.search_stops("Lyon", 5);
        assert!(!results.is_empty());
    }

    #[test]
    fn search_stops_empty_query() {
        let data = build_test_data();
        let results = data.search_stops("", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn search_stops_no_match() {
        let data = build_test_data();
        let results = data.search_stops("zzzznonexistent", 5);
        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_stop
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_stop_by_id() {
        let data = build_test_data();
        let idx = data.resolve_stop("S1", u32::MAX);
        assert!(idx.is_some());
        assert_eq!(idx.unwrap(), data.stop_index["S1"]);
    }

    #[test]
    fn resolve_stop_by_coords() {
        let data = build_test_data();
        // lon;lat near S1 (2.347;48.858)
        let idx = data.resolve_stop("2.347;48.858", u32::MAX);
        assert!(idx.is_some());
    }

    #[test]
    fn resolve_stop_unknown() {
        let data = build_test_data();
        assert!(data.resolve_stop("UNKNOWN_STOP", u32::MAX).is_none());
    }

    // -----------------------------------------------------------------------
    // find_earliest_trip
    // -----------------------------------------------------------------------

    #[test]
    fn find_earliest_trip_basic() {
        let data = build_test_data();
        let pattern = &data.patterns[0];
        let active = data.active_services("20260406"); // Monday, SVC1 active
        // At position 0, departure 08:01:00 = 28860, look for trip departing >= 28000
        let result = find_earliest_trip(pattern, 0, 28000, &active);
        assert!(result.is_some());
    }

    #[test]
    fn find_earliest_trip_too_late() {
        let data = build_test_data();
        let pattern = &data.patterns[0];
        let active = data.active_services("20260406");
        // All trips depart before 40000 at pos 0 → should return the last trip
        let result = find_earliest_trip(pattern, 0, 99999, &active);
        assert!(result.is_none());
    }

    #[test]
    fn find_earliest_trip_inactive_service() {
        let data = build_test_data();
        let pattern = &data.patterns[0];
        let active = data.active_services("20260101"); // exception: SVC1 removed
        let result = find_earliest_trip(pattern, 0, 0, &active);
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // raptor_query + reconstruct_journeys
    // -----------------------------------------------------------------------

    #[test]
    fn raptor_query_simple() {
        let data = build_test_data();
        let source = data.stop_index["S1"];
        let target = data.stop_index["S3"];
        let active = data.active_services("20260406");
        let result = raptor_query(
            &data,
            &[(source, 0)],
            28000,
            &active,
            3,
            &FxHashSet::default(),
        );
        let journeys = reconstruct_journeys(&data, &result, &[(target, 0)]);
        assert!(!journeys.is_empty());
    }

    #[test]
    fn raptor_query_same_stop() {
        let data = build_test_data();
        let source = data.stop_index["S1"];
        let active = data.active_services("20260406");
        let result = raptor_query(
            &data,
            &[(source, 0)],
            28000,
            &active,
            3,
            &FxHashSet::default(),
        );
        // Source is already at target → tau[0][source] == departure_time
        assert_eq!(result.tau[0][source], 28000);
    }

    #[test]
    fn raptor_query_no_service() {
        let data = build_test_data();
        let source = data.stop_index["S1"];
        let target = data.stop_index["S3"];
        let active = data.active_services("20260101"); // no service
        let result = raptor_query(
            &data,
            &[(source, 0)],
            28000,
            &active,
            3,
            &FxHashSet::default(),
        );
        let journeys = reconstruct_journeys(&data, &result, &[(target, 0)]);
        assert!(journeys.is_empty());
    }

    #[test]
    fn raptor_query_with_excluded_patterns() {
        let data = build_test_data();
        let source = data.stop_index["S1"];
        let target = data.stop_index["S3"];
        let active = data.active_services("20260406");
        // Exclude all patterns
        let excluded: FxHashSet<usize> = (0..data.patterns.len()).collect();
        let result = raptor_query(&data, &[(source, 0)], 28000, &active, 3, &excluded);
        let journeys = reconstruct_journeys(&data, &result, &[(target, 0)]);
        assert!(journeys.is_empty());
    }

    // -----------------------------------------------------------------------
    // used_patterns
    // -----------------------------------------------------------------------

    #[test]
    fn used_patterns_extracts_pattern_indices() {
        let sections = vec![
            JourneySection {
                section_type: SectionType::PublicTransport,
                from_stop: 0,
                to_stop: 1,
                departure_time: 100,
                arrival_time: 200,
                pattern_idx: Some(5),
                trip_idx: Some(0),
                board_pos: Some(0),
                alight_pos: Some(1),
            },
            JourneySection {
                section_type: SectionType::Transfer,
                from_stop: 1,
                to_stop: 2,
                departure_time: 200,
                arrival_time: 300,
                pattern_idx: None,
                trip_idx: None,
                board_pos: None,
                alight_pos: None,
            },
        ];
        let pats = used_patterns(&sections);
        assert_eq!(pats.len(), 1);
        assert!(pats.contains(&5));
    }

    // -----------------------------------------------------------------------
    // Cache persistence
    // -----------------------------------------------------------------------

    #[test]
    fn save_and_load_cache() {
        let data = build_test_data();
        let dir = tempfile::tempdir().unwrap();
        let fp = "test_fp_123";
        data.save(dir.path(), fp).unwrap();

        let loaded = RaptorData::load_cached(dir.path(), fp);
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.stops.len(), data.stops.len());
        assert_eq!(loaded.patterns.len(), data.patterns.len());
    }

    #[test]
    fn load_cache_wrong_fingerprint() {
        let data = build_test_data();
        let dir = tempfile::tempdir().unwrap();
        data.save(dir.path(), "fp1").unwrap();

        let loaded = RaptorData::load_cached(dir.path(), "fp_different");
        assert!(loaded.is_none());
    }

    #[test]
    fn load_cache_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = RaptorData::load_cached(dir.path(), "anything");
        assert!(loaded.is_none());
    }

    // -----------------------------------------------------------------------
    // sanitize_sections
    // -----------------------------------------------------------------------

    #[test]
    fn sanitize_removes_self_loop_transfer() {
        let data = build_test_data();
        let sections = vec![JourneySection {
            section_type: SectionType::Transfer,
            from_stop: 5,
            to_stop: 5,
            departure_time: 100,
            arrival_time: 100,
            pattern_idx: None,
            trip_idx: None,
            board_pos: None,
            alight_pos: None,
        }];
        let clean = sanitize_sections(&data, sections);
        assert!(clean.is_empty());
    }

    #[test]
    fn sanitize_removes_zero_duration_pt() {
        let data = build_test_data();
        let sections = vec![JourneySection {
            section_type: SectionType::PublicTransport,
            from_stop: 0,
            to_stop: 1,
            departure_time: 100,
            arrival_time: 100, // zero duration
            pattern_idx: Some(0),
            trip_idx: Some(0),
            board_pos: Some(0),
            alight_pos: Some(1),
        }];
        let clean = sanitize_sections(&data, sections);
        assert!(clean.is_empty());
    }

    #[test]
    fn sanitize_merges_consecutive_transfers() {
        let data = build_test_data();
        let sections = vec![
            JourneySection {
                section_type: SectionType::Transfer,
                from_stop: 0,
                to_stop: 1,
                departure_time: 100,
                arrival_time: 200,
                pattern_idx: None,
                trip_idx: None,
                board_pos: None,
                alight_pos: None,
            },
            JourneySection {
                section_type: SectionType::Transfer,
                from_stop: 1,
                to_stop: 2,
                departure_time: 200,
                arrival_time: 300,
                pattern_idx: None,
                trip_idx: None,
                board_pos: None,
                alight_pos: None,
            },
        ];
        let clean = sanitize_sections(&data, sections);
        assert_eq!(clean.len(), 1);
        assert_eq!(clean[0].from_stop, 0);
        assert_eq!(clean[0].to_stop, 2);
        assert_eq!(clean[0].arrival_time, 300);
    }

    // -----------------------------------------------------------------------
    // active_services — weekday coverage
    // -----------------------------------------------------------------------

    #[test]
    fn active_services_each_weekday() {
        let data = build_test_data();
        let svc_idx = data.service_ids.iter().position(|s| s == "SVC1").unwrap();
        // SVC1 has all days = 1, valid 20260101-20261231
        // Test each weekday within range (week of 2026-04-06 Mon to 2026-04-12 Sun)
        for date in &[
            "20260406", "20260407", "20260408", "20260409", "20260410", "20260411", "20260412",
        ] {
            let active = data.active_services(date);
            assert!(active[svc_idx], "SVC1 should be active on {date}");
        }
    }

    #[test]
    fn active_services_no_calendar_no_exception() {
        // Service ID that exists in neither calendar nor calendar_dates
        let mut gtfs = make_test_gtfs();
        gtfs.trips.insert(
            "T_ORPHAN".to_string(),
            gtfs::Trip {
                route_id: "R1".to_string(),
                service_id: "SVC_ORPHAN".to_string(),
                trip_id: "T_ORPHAN".to_string(),
                trip_headsign: "Nowhere".to_string(),
            },
        );
        let data = RaptorData::build(gtfs, 120);
        let svc_idx = data
            .service_ids
            .iter()
            .position(|s| s == "SVC_ORPHAN")
            .unwrap();
        let active = data.active_services("20260406");
        assert!(!active[svc_idx]);
    }

    #[test]
    fn active_services_exception_added() {
        let mut gtfs = make_test_gtfs();
        // Add a service that only runs via exception (no calendar entry)
        gtfs.calendar_dates.push(gtfs::CalendarDate {
            service_id: "SVC_SPECIAL".to_string(),
            date: "20260501".to_string(),
            exception_type: 1, // added
        });
        let data = RaptorData::build(gtfs, 120);
        let svc_idx = data
            .service_ids
            .iter()
            .position(|s| s == "SVC_SPECIAL")
            .unwrap();
        let active = data.active_services("20260501");
        assert!(active[svc_idx]);
        // Different date → not active (no calendar, no exception)
        let active2 = data.active_services("20260502");
        assert!(!active2[svc_idx]);
    }

    // -----------------------------------------------------------------------
    // raptor_query — transfer via improved stops
    // -----------------------------------------------------------------------

    #[test]
    fn raptor_query_uses_transfers() {
        let data = build_test_data();
        let source = data.stop_index["S1"];
        let target = data.stop_index["S3"];
        let active = data.active_services("20260406");
        let result = raptor_query(
            &data,
            &[(source, 0)],
            28000,
            &active,
            3,
            &FxHashSet::default(),
        );
        let journeys = reconstruct_journeys(&data, &result, &[(target, 0)]);
        assert!(!journeys.is_empty());
        // Journey should have at least one PT section
        let has_pt = journeys[0]
            .iter()
            .any(|s| matches!(s.section_type, SectionType::PublicTransport));
        assert!(has_pt);
    }

    #[test]
    fn raptor_query_departure_after_last_trip() {
        let data = build_test_data();
        let source = data.stop_index["S1"];
        let target = data.stop_index["S3"];
        let active = data.active_services("20260406");
        // Departure at 23:00 — after all trips (last departs at 09:01)
        let result = raptor_query(
            &data,
            &[(source, 0)],
            82800,
            &active,
            3,
            &FxHashSet::default(),
        );
        let journeys = reconstruct_journeys(&data, &result, &[(target, 0)]);
        assert!(journeys.is_empty());
    }

    // -----------------------------------------------------------------------
    // format_datetime — edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn format_datetime_invalid_date() {
        // Short date string — should fallback gracefully
        let result = format_datetime("bad", 90000);
        assert!(result.contains("T"));
    }

    #[test]
    fn format_datetime_multi_day_rollover() {
        // 48h = 172800s → +2 days
        assert_eq!(format_datetime("20260405", 172800), "20260407T000000");
    }

    // -----------------------------------------------------------------------
    // sanitize_sections — PT from==to
    // -----------------------------------------------------------------------

    #[test]
    fn sanitize_removes_pt_same_stop() {
        let data = build_test_data();
        let sections = vec![JourneySection {
            section_type: SectionType::PublicTransport,
            from_stop: 3,
            to_stop: 3, // same stop
            departure_time: 100,
            arrival_time: 200,
            pattern_idx: Some(0),
            trip_idx: Some(0),
            board_pos: Some(0),
            alight_pos: Some(0),
        }];
        let clean = sanitize_sections(&data, sections);
        assert!(clean.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_stop — nearest stop by coords
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_stop_nearest_returns_closest() {
        let data = build_test_data();
        // Coords very close to S2 (lon=2.373, lat=48.844)
        let idx = data.resolve_stop("2.374;48.845", u32::MAX).unwrap();
        let s2 = data.stop_index["S2"];
        assert_eq!(idx, s2);
    }

    #[test]
    fn resolve_stop_invalid_coords() {
        let data = build_test_data();
        assert!(data.resolve_stop("notlon;notlat", u32::MAX).is_none());
    }

    // -----------------------------------------------------------------------
    // search_stops — limit
    // -----------------------------------------------------------------------

    #[test]
    fn search_stops_respects_limit() {
        let data = build_test_data();
        let results = data.search_stops("a", 1);
        assert!(results.len() <= 1);
    }

    // -----------------------------------------------------------------------
    // Cache — corrupted data
    // -----------------------------------------------------------------------

    #[test]
    fn load_cache_corrupted_data() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("raptor.fingerprint"), "fp1").unwrap();
        std::fs::write(dir.path().join("raptor.bin"), b"not valid bincode").unwrap();
        let loaded = RaptorData::load_cached(dir.path(), "fp1");
        assert!(loaded.is_none());
    }
}
