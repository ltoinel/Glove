//! Resolves the catalog against the schedule into an index the router can read.
//!
//! Same shape as [`crate::transit::realtime::index`], and for the same reason:
//! the RAPTOR index takes 10-30 s to build, so a disruption must never trigger
//! a rebuild. Operators author stop and route *identifiers*; the router works
//! in stop and pattern *indices*. Translating between the two is all this
//! module does, once per query rather than once per pattern scan.
//!
//! Resolution is time-dependent — a disruption applies only while its period
//! covers the query instant — so the index is built per query. That costs one
//! pass over the catalog, which holds tens to hundreds of entries.

use rustc_hash::{FxHashMap, FxHashSet};

use super::model::{Disruption, Scope, Severity};
use super::store::Catalog;
use crate::transit::raptor::{JourneySection, RaptorData};

/// What the router and the API need to know about the disruptions in force.
///
/// Every lookup answers two different questions: *is this blocked* (routing)
/// and *what should I tell the user* (reporting). Both are served from the
/// same maps, with severity filtering applied at the call site, so a blocking
/// and an informational disruption on the same stop cannot drift apart.
#[derive(Debug, Default)]
pub struct DisruptionIndex {
    /// Disruptions in force at the query instant, in catalog order.
    entries: Vec<Disruption>,
    /// `stop_idx` → indices into [`Self::entries`] that name this stop.
    stops: FxHashMap<u32, Vec<u32>>,
    /// `pattern_idx` → entries disrupting the whole line.
    patterns: FxHashMap<u32, Vec<u32>>,
    /// `(pattern_idx, position)` → entries cutting the ride from `position` to
    /// `position + 1`.
    rides: FxHashMap<(u32, u32), Vec<u32>>,
    /// Patterns removed outright, ready to union into the router's exclusion
    /// set. Only whole-line blocking disruptions land here — a blocked section
    /// keeps the rest of the line usable.
    blocked_patterns: FxHashSet<usize>,
    /// Whether any entry has [`Severity::Blocking`]. Lets callers skip the
    /// second RAPTOR pass entirely when nothing blocks.
    has_blocking: bool,
}

impl DisruptionIndex {
    /// Whether the index holds nothing at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether any disruption in force actually removes something.
    pub fn has_blocking(&self) -> bool {
        self.has_blocking
    }

    /// Patterns to add to the router's exclusion set.
    pub fn blocked_patterns(&self) -> &FxHashSet<usize> {
        &self.blocked_patterns
    }

    /// Whether the stop is unusable: no boarding, no alighting, no transfer.
    pub fn blocks_stop(&self, stop_idx: usize) -> bool {
        self.any_blocking(self.stops.get(&(stop_idx as u32)))
    }

    /// Whether staying aboard from `position` to `position + 1` is cut.
    pub fn blocks_ride(&self, pattern_idx: usize, position: usize) -> bool {
        self.any_blocking(self.rides.get(&(pattern_idx as u32, position as u32)))
    }

    /// Entries naming this stop, whatever their severity.
    pub fn stop_entries(&self, stop_idx: usize) -> &[u32] {
        Self::slice(self.stops.get(&(stop_idx as u32)))
    }

    /// Entries naming the line this pattern belongs to.
    pub fn pattern_entries(&self, pattern_idx: usize) -> &[u32] {
        Self::slice(self.patterns.get(&(pattern_idx as u32)))
    }

    /// Entries cutting the ride from `position` to `position + 1`.
    pub fn ride_entries(&self, pattern_idx: usize, position: usize) -> &[u32] {
        Self::slice(self.rides.get(&(pattern_idx as u32, position as u32)))
    }

    /// One entry by index, as handed out by the `*_entries` accessors.
    pub fn entry(&self, index: u32) -> Option<&Disruption> {
        self.entries.get(index as usize)
    }

    fn any_blocking(&self, indices: Option<&Vec<u32>>) -> bool {
        indices.is_some_and(|list| {
            list.iter().any(|&i| {
                self.entries
                    .get(i as usize)
                    .is_some_and(|d| d.severity == Severity::Blocking)
            })
        })
    }

    fn slice(indices: Option<&Vec<u32>>) -> &[u32] {
        indices.map_or(&[], Vec::as_slice)
    }
}

/// What one disruption removes from the network, in map terms.
///
/// Edges rather than whole stop sequences: a line closure blocks every ride of
/// every pattern of that route, and those patterns overlap heavily. Collapsing
/// to a deduplicated edge set keeps the payload bounded by the network's
/// topology instead of by its pattern count.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BlockedGeometry {
    /// Stop indices the disruption closes.
    pub stops: Vec<usize>,
    /// `(from_stop_idx, to_stop_idx)` rides it cuts, direction-normalized.
    pub edges: Vec<(usize, usize)>,
}

/// The geometry each blocking disruption removes, indexed like
/// [`DisruptionIndex::entry`].
///
/// Informational disruptions are skipped: they remove nothing, so drawing them
/// as blockages would misinform.
pub fn blocked_geometry(index: &DisruptionIndex, data: &RaptorData) -> Vec<(u32, BlockedGeometry)> {
    let mut per_entry: Vec<BlockedGeometry> = index
        .entries
        .iter()
        .map(|_| BlockedGeometry::default())
        .collect();

    let is_blocking = |entry: u32| {
        index
            .entries
            .get(entry as usize)
            .is_some_and(|d| d.severity == Severity::Blocking)
    };

    for (&stop_idx, entries) in &index.stops {
        for &entry in entries.iter().filter(|&&e| is_blocking(e)) {
            per_entry[entry as usize].stops.push(stop_idx as usize);
        }
    }

    // A whole-line closure cuts every ride of every pattern it serves.
    for (&pattern_idx, entries) in &index.patterns {
        let stops = &data.patterns[pattern_idx as usize].stops;
        for &entry in entries.iter().filter(|&&e| is_blocking(e)) {
            for pair in stops.windows(2) {
                per_entry[entry as usize]
                    .edges
                    .push(normalize_edge(pair[0], pair[1]));
            }
        }
    }

    // A section closure cuts only the rides it spans.
    for (&(pattern_idx, position), entries) in &index.rides {
        let stops = &data.patterns[pattern_idx as usize].stops;
        let (Some(&from), Some(&to)) = (
            stops.get(position as usize),
            stops.get(position as usize + 1),
        ) else {
            continue;
        };
        for &entry in entries.iter().filter(|&&e| is_blocking(e)) {
            per_entry[entry as usize]
                .edges
                .push(normalize_edge(from, to));
        }
    }

    per_entry
        .into_iter()
        .enumerate()
        .filter_map(|(entry, mut geometry)| {
            geometry.stops.sort_unstable();
            geometry.stops.dedup();
            geometry.edges.sort_unstable();
            geometry.edges.dedup();
            let empty = geometry.stops.is_empty() && geometry.edges.is_empty();
            (!empty).then_some((entry as u32, geometry))
        })
        .collect()
}

/// Order-independent edge key: the map draws an undisrupted segment the same
/// way in either direction, so both directions must collapse to one entry.
fn normalize_edge(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

/// One disruption touching one section of a reconstructed journey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impact {
    /// Position of the section within the journey.
    pub section: usize,
    /// Index into [`DisruptionIndex::entries`].
    pub entry: u32,
    /// The stop the disruption closes, when it names one. `None` for a line
    /// or section closure, which has no single stop to point at.
    pub stop_idx: Option<usize>,
}

/// Every disruption touching `sections`, whatever its severity.
///
/// Reported per section rather than per journey so the portal can point at the
/// leg that fails instead of showing a message next to the whole result. A
/// disruption hitting several stops of the same leg is reported once, on the
/// first stop it closes.
pub fn journey_impacts(index: &DisruptionIndex, sections: &[JourneySection]) -> Vec<Impact> {
    let mut impacts: Vec<Impact> = Vec::new();

    for (position, section) in sections.iter().enumerate() {
        let mut record = |entry: u32, stop_idx: Option<usize>| {
            if impacts
                .iter()
                .any(|i| i.section == position && i.entry == entry)
            {
                return;
            }
            impacts.push(Impact {
                section: position,
                entry,
                stop_idx,
            });
        };

        match (section.pattern_idx, section.board_pos, section.alight_pos) {
            (Some(pattern_idx), Some(board), Some(alight)) => {
                for &entry in index.pattern_entries(pattern_idx) {
                    record(entry, None);
                }
                for ride in board..alight {
                    for &entry in index.ride_entries(pattern_idx, ride) {
                        record(entry, None);
                    }
                }
                for &entry in index.stop_entries(section.from_stop) {
                    record(entry, Some(section.from_stop));
                }
                for &entry in index.stop_entries(section.to_stop) {
                    record(entry, Some(section.to_stop));
                }
            }
            // A transfer only ever touches its two endpoints.
            _ => {
                for &entry in index.stop_entries(section.from_stop) {
                    record(entry, Some(section.from_stop));
                }
                for &entry in index.stop_entries(section.to_stop) {
                    record(entry, Some(section.to_stop));
                }
            }
        }
    }

    impacts
}

/// Build the index of disruptions in force at `instant`.
pub fn resolve(
    data: &RaptorData,
    catalog: &Catalog,
    instant: chrono::NaiveDateTime,
) -> DisruptionIndex {
    let in_force: Vec<&Disruption> = catalog
        .disruptions
        .iter()
        .filter(|d| d.period.covers(instant))
        .collect();

    let mut index = DisruptionIndex::default();
    if in_force.is_empty() {
        return index;
    }

    // Built lazily: most catalogs name a handful of lines, and walking every
    // pattern once is cheaper than walking them once per disruption.
    let route_patterns = build_route_pattern_index(data);

    for disruption in in_force {
        let entry_idx = index.entries.len() as u32;
        let blocking = disruption.severity == Severity::Blocking;

        match &disruption.scope {
            Scope::Stop { stop_id } => {
                apply_stop_scope(data, stop_id, entry_idx, &mut index);
            }
            Scope::Line { route_id } => {
                apply_line_scope(&route_patterns, route_id, entry_idx, blocking, &mut index);
            }
            Scope::LineSection {
                route_id,
                from_stop_id,
                to_stop_id,
            } => {
                apply_section_scope(
                    data,
                    &route_patterns,
                    route_id,
                    from_stop_id,
                    to_stop_id,
                    entry_idx,
                    &mut index,
                );
            }
        }

        index.has_blocking |= blocking;
        index.entries.push(disruption.clone());
    }

    index
}

/// Neutralize every stop belonging to the named station.
fn apply_stop_scope(data: &RaptorData, stop_id: &str, entry_idx: u32, index: &mut DisruptionIndex) {
    for stop_idx in expand_station(data, stop_id) {
        index
            .stops
            .entry(stop_idx as u32)
            .or_default()
            .push(entry_idx);
    }
}

/// Mark every pattern of the line, and exclude them outright when blocking.
fn apply_line_scope(
    route_patterns: &FxHashMap<&str, Vec<usize>>,
    route_id: &str,
    entry_idx: u32,
    blocking: bool,
    index: &mut DisruptionIndex,
) {
    let Some(patterns) = route_patterns.get(route_id) else {
        return;
    };
    for &pattern_idx in patterns {
        index
            .patterns
            .entry(pattern_idx as u32)
            .or_default()
            .push(entry_idx);
        if blocking {
            index.blocked_patterns.insert(pattern_idx);
        }
    }
}

/// Cut every ride between the two endpoints, in whichever order the pattern
/// visits them — a section closure applies to both directions of the line.
fn apply_section_scope(
    data: &RaptorData,
    route_patterns: &FxHashMap<&str, Vec<usize>>,
    route_id: &str,
    from_stop_id: &str,
    to_stop_id: &str,
    entry_idx: u32,
    index: &mut DisruptionIndex,
) {
    let Some(patterns) = route_patterns.get(route_id) else {
        return;
    };
    let from_stops = expand_station(data, from_stop_id);
    let to_stops = expand_station(data, to_stop_id);
    if from_stops.is_empty() || to_stops.is_empty() {
        return;
    }

    for &pattern_idx in patterns {
        let Some((low, high)) =
            section_bounds(&data.patterns[pattern_idx].stops, &from_stops, &to_stops)
        else {
            continue;
        };
        for position in low..high {
            index
                .rides
                .entry((pattern_idx as u32, position as u32))
                .or_default()
                .push(entry_idx);
        }
    }
}

/// Positions spanned by the section within one pattern, as `[low, high)` ride
/// offsets. `None` when the pattern does not serve both endpoints.
fn section_bounds(
    stops: &[usize],
    from_stops: &FxHashSet<usize>,
    to_stops: &FxHashSet<usize>,
) -> Option<(usize, usize)> {
    let first = stops.iter().position(|s| from_stops.contains(s));
    let second = stops.iter().position(|s| to_stops.contains(s));
    let (first, second) = (first?, second?);
    if first == second {
        return None;
    }
    Some((first.min(second), first.max(second)))
}

/// Every stop index belonging to the same station as `stop_id`.
///
/// Deliberately wider than journey origin resolution: closing "Châtelet" must
/// close its platforms too, so the given id is expanded upward to its parent
/// station and downward to every child.
fn expand_station(data: &RaptorData, stop_id: &str) -> FxHashSet<usize> {
    let mut result = FxHashSet::default();
    let Some(&idx) = data.stop_index.get(stop_id) else {
        return result;
    };
    result.insert(idx);

    // The named stop may itself be a station node: take everything under it.
    for (other_idx, stop) in data.stops.iter().enumerate() {
        if stop.parent_station == stop_id {
            result.insert(other_idx);
        }
    }

    // Or a platform: take the station and its siblings.
    let parent = &data.stops[idx].parent_station;
    if !parent.is_empty() {
        if let Some(&parent_idx) = data.stop_index.get(parent.as_str()) {
            result.insert(parent_idx);
        }
        for (other_idx, stop) in data.stops.iter().enumerate() {
            if &stop.parent_station == parent {
                result.insert(other_idx);
            }
        }
    }

    result
}

/// `route_id` → the patterns that serve it.
fn build_route_pattern_index(data: &RaptorData) -> FxHashMap<&str, Vec<usize>> {
    let mut map: FxHashMap<&str, Vec<usize>> = FxHashMap::default();
    for (pattern_idx, pattern) in data.patterns.iter().enumerate() {
        map.entry(pattern.route_id.as_str())
            .or_default()
            .push(pattern_idx);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transit::disruptions::model::{Cause, Period, Severity};
    use crate::transit::raptor::test_support::build_test_data;

    fn at(text: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").expect("valid test time")
    }

    fn disruption(id: &str, scope: Scope, severity: Severity) -> Disruption {
        Disruption {
            id: id.to_string(),
            title: "Travaux".to_string(),
            message: String::new(),
            cause: Cause::Works,
            severity,
            scope,
            period: Period {
                starts_at: at("2026-09-01T00:00:00"),
                ends_at: Some(at("2026-09-02T00:00:00")),
            },
            created_at: at("2026-08-01T00:00:00"),
            updated_at: at("2026-08-01T00:00:00"),
        }
    }

    fn catalog(disruptions: Vec<Disruption>) -> Catalog {
        let json = serde_json::to_string(&serde_json::json!({
            "next_id": disruptions.len(),
            "disruptions": disruptions,
        }))
        .expect("serialize catalog");
        serde_json::from_str(&json).expect("deserialize catalog")
    }

    #[test]
    fn a_period_outside_the_query_instant_resolves_to_nothing() {
        let data = build_test_data();
        let cat = catalog(vec![disruption(
            "d1",
            Scope::Stop {
                stop_id: "S2".into(),
            },
            Severity::Blocking,
        )]);
        let index = resolve(&data, &cat, at("2026-09-03T10:00:00"));
        assert!(index.is_empty());
        assert!(!index.has_blocking());
    }

    #[test]
    fn a_blocking_stop_disruption_marks_the_stop() {
        let data = build_test_data();
        let s2 = data.stop_index["S2"];
        let cat = catalog(vec![disruption(
            "d1",
            Scope::Stop {
                stop_id: "S2".into(),
            },
            Severity::Blocking,
        )]);
        let index = resolve(&data, &cat, at("2026-09-01T10:00:00"));

        assert!(index.blocks_stop(s2));
        assert!(index.has_blocking());
        assert_eq!(index.stop_entries(s2).len(), 1);
        assert_eq!(
            index.entry(index.stop_entries(s2)[0]).expect("entry").id,
            "d1"
        );
    }

    #[test]
    fn an_info_stop_disruption_reports_without_blocking() {
        let data = build_test_data();
        let s2 = data.stop_index["S2"];
        let cat = catalog(vec![disruption(
            "d1",
            Scope::Stop {
                stop_id: "S2".into(),
            },
            Severity::Info,
        )]);
        let index = resolve(&data, &cat, at("2026-09-01T10:00:00"));

        assert!(!index.blocks_stop(s2));
        assert!(!index.has_blocking());
        assert_eq!(index.stop_entries(s2).len(), 1);
    }

    #[test]
    fn a_blocking_line_disruption_excludes_every_pattern_of_that_route() {
        let data = build_test_data();
        let route_id = data.patterns[0].route_id.clone();
        let expected: Vec<usize> = data
            .patterns
            .iter()
            .enumerate()
            .filter(|(_, p)| p.route_id == route_id)
            .map(|(i, _)| i)
            .collect();

        let cat = catalog(vec![disruption(
            "d1",
            Scope::Line {
                route_id: route_id.clone(),
            },
            Severity::Blocking,
        )]);
        let index = resolve(&data, &cat, at("2026-09-01T10:00:00"));

        for pattern_idx in &expected {
            assert!(index.blocked_patterns().contains(pattern_idx));
            assert_eq!(index.pattern_entries(*pattern_idx).len(), 1);
        }
    }

    #[test]
    fn an_info_line_disruption_reports_without_excluding() {
        let data = build_test_data();
        let route_id = data.patterns[0].route_id.clone();
        let cat = catalog(vec![disruption(
            "d1",
            Scope::Line { route_id },
            Severity::Info,
        )]);
        let index = resolve(&data, &cat, at("2026-09-01T10:00:00"));

        assert!(index.blocked_patterns().is_empty());
        assert_eq!(index.pattern_entries(0).len(), 1);
    }

    #[test]
    fn a_section_cuts_only_the_rides_it_spans() {
        let data = build_test_data();
        let pattern = &data.patterns[0];
        assert!(
            pattern.stops.len() >= 3,
            "test fixture needs a pattern of at least three stops"
        );
        let from_id = data.stops[pattern.stops[0]].stop_id.clone();
        let to_id = data.stops[pattern.stops[1]].stop_id.clone();
        let route_id = pattern.route_id.clone();

        let cat = catalog(vec![disruption(
            "d1",
            Scope::LineSection {
                route_id,
                from_stop_id: from_id,
                to_stop_id: to_id,
            },
            Severity::Blocking,
        )]);
        let index = resolve(&data, &cat, at("2026-09-01T10:00:00"));

        assert!(index.blocks_ride(0, 0), "the disrupted ride must be cut");
        assert!(
            !index.blocks_ride(0, 1),
            "rides beyond the section must stay usable"
        );
        assert!(
            index.blocked_patterns().is_empty(),
            "a section closure must not exclude the whole line"
        );
    }

    #[test]
    fn an_unknown_identifier_resolves_to_an_entry_that_matches_nothing() {
        let data = build_test_data();
        let cat = catalog(vec![disruption(
            "d1",
            Scope::Line {
                route_id: "does-not-exist".into(),
            },
            Severity::Blocking,
        )]);
        let index = resolve(&data, &cat, at("2026-09-01T10:00:00"));

        assert!(index.entry(0).is_some());
        assert!(index.blocked_patterns().is_empty());
    }

    #[test]
    fn a_platform_closure_expands_to_its_siblings() {
        let data = build_test_data();
        // S1 carries parent_station "P1" in the fixture; closing it must also
        // close anything else under P1.
        let s1 = data.stop_index["S1"];
        let parent = data.stops[s1].parent_station.clone();
        assert!(!parent.is_empty(), "fixture must have a parented stop");

        let cat = catalog(vec![disruption(
            "d1",
            Scope::Stop {
                stop_id: "S1".into(),
            },
            Severity::Blocking,
        )]);
        let index = resolve(&data, &cat, at("2026-09-01T10:00:00"));

        for (idx, stop) in data.stops.iter().enumerate() {
            if stop.parent_station == parent {
                assert!(
                    index.blocks_stop(idx),
                    "sibling {} must be closed too",
                    stop.stop_id
                );
            }
        }
    }
}
