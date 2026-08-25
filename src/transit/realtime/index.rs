//! Resolves decoded feeds against the RAPTOR index, producing an overlay the
//! router can consult without rebuilding anything.
//!
//! Rebuilding [`RaptorData`] on every refresh is out of the question: the
//! pre-processing step takes 10-30 s, and a feed refreshes every 30 s. Instead
//! a [`RealtimeIndex`] is resolved once per refresh — feed identifiers into
//! `(pattern_idx, trip_idx)` pairs, predictions into per-call offsets — and
//! published atomically. The router then reads schedule + overlay.
//!
//! Two mapping problems are handled here:
//!
//! - **Trips.** Feeds key updates by `trip_id`. [`TripLookup`] inverts the
//!   pattern index once per GTFS load to resolve them.
//! - **Calls.** `build_patterns` sorts calls by `stop_sequence` but does not
//!   keep the original values, so a position cannot be recovered from
//!   `stop_sequence` alone. Calls are matched by `stop_id` with a forward-only
//!   cursor, which also keeps loop routes (a stop served twice) in order.

use chrono::{Local, NaiveDate, TimeZone};
use rustc_hash::FxHashMap;

use super::model::{RealtimeFeed, StopRelationship, TimeUpdate, TripRelationship, TripUpdate};
use crate::transit::raptor::{Pattern, RaptorData};

/// Offsets applied to one call of one trip, in seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallDelta {
    /// Seconds to add to the scheduled arrival (negative = running early).
    pub arrival: i32,
    /// Seconds to add to the scheduled departure.
    pub departure: i32,
    /// The vehicle does not call here on this run.
    pub skipped: bool,
}

/// Real-time adjustments for one trip.
#[derive(Debug, Clone, Default)]
pub struct TripDelta {
    /// The trip does not run: the router must ignore it entirely.
    pub canceled: bool,
    /// The vehicle passes at least one scheduled call without stopping.
    ///
    /// RAPTOR groups trips by stop sequence, so a trip that drops a call no
    /// longer matches the pattern it is filed under. Splitting the pattern per
    /// refresh is not an option — that is the pre-processing this whole overlay
    /// exists to avoid — so the router treats such a trip like a cancelled one.
    /// That is pessimistic for passengers travelling to the *other* stops of
    /// the trip, and safe for everyone: nobody is ever routed to alight at a
    /// call the vehicle skips, and the journey falls back to the next service
    /// instead of disappearing.
    pub skips_calls: bool,
    /// One entry per position in the pattern. Empty when the only information
    /// is the cancellation.
    calls: Vec<CallDelta>,
}

impl TripDelta {
    /// Real-time arrival at `pos`, or `None` if the vehicle skips this call.
    pub fn arrival(&self, pos: usize, scheduled: u32) -> Option<u32> {
        let delta = self.call(pos)?;
        Some(shift(scheduled, delta.arrival))
    }

    /// Real-time departure at `pos`, or `None` if the vehicle skips this call.
    pub fn departure(&self, pos: usize, scheduled: u32) -> Option<u32> {
        let delta = self.call(pos)?;
        Some(shift(scheduled, delta.departure))
    }

    fn call(&self, pos: usize) -> Option<&CallDelta> {
        match self.calls.get(pos) {
            Some(delta) if delta.skipped => None,
            Some(delta) => Some(delta),
            // No entry means no prediction for this call: the schedule stands.
            None => Some(&CallDelta {
                arrival: 0,
                departure: 0,
                skipped: false,
            }),
        }
    }
}

/// Apply a signed offset to a schedule time without wrapping past zero.
fn shift(scheduled: u32, delta: i32) -> u32 {
    if delta >= 0 {
        scheduled.saturating_add(delta as u32)
    } else {
        scheduled.saturating_sub(delta.unsigned_abs())
    }
}

/// Real-time adjustments for every updated trip of one pattern.
#[derive(Debug, Default)]
pub struct PatternDeltas {
    trips: FxHashMap<u32, TripDelta>,
    max_abs_delta: u32,
}

impl PatternDeltas {
    /// Adjustments for `trip_idx`, or `None` when that trip has no update.
    pub fn trip(&self, trip_idx: usize) -> Option<&TripDelta> {
        self.trips.get(&(trip_idx as u32))
    }

    /// Largest absolute offset seen on this pattern.
    ///
    /// Trips are stored sorted by *scheduled* departure, an order real-time
    /// offsets can break. Callers that binary-search on the schedule use this
    /// to widen their window by as much as reality has diverged from it.
    pub fn max_abs_delta(&self) -> u32 {
        self.max_abs_delta
    }
}

/// How much of the last refresh could be applied. Surfaced by
/// `GET /api/realtime/status`, because a silent drop to zero matches is
/// otherwise indistinguishable from a network running perfectly on time.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, utoipa::ToSchema)]
pub struct MatchStats {
    /// Trip updates resolved to a scheduled trip.
    pub matched_trips: usize,
    /// Trip updates whose `trip_id` is absent from the GTFS schedule.
    pub unmatched_trips: usize,
    /// Trips reported as cancelled.
    pub canceled_trips: usize,
    /// Updates for trips that do not exist in the schedule (ADDED and friends),
    /// which phase 1 records but does not route over.
    pub unsupported_trips: usize,
    /// Calls resolved to a position in their pattern.
    pub matched_calls: usize,
    /// Calls whose `stop_id` is not served by the resolved pattern.
    pub unmatched_calls: usize,
    /// Calls located by `stop_sequence` because the feed sent no `stop_id`.
    pub sequence_fallback_calls: usize,
    /// Predictions given as an absolute time that could not be converted into
    /// an offset (unparseable service date).
    pub unresolved_times: usize,
}

/// The overlay consulted by the router.
///
/// Indexed by `pattern_idx` so the hot loop pays one bounds-checked lookup to
/// learn a pattern has no real-time data at all — the common case.
#[derive(Debug, Default)]
pub struct RealtimeIndex {
    patterns: Vec<Option<Box<PatternDeltas>>>,
    pub stats: MatchStats,
}

impl RealtimeIndex {
    /// Adjustments for `pattern_idx`, or `None` when the pattern is untouched.
    pub fn pattern(&self, pattern_idx: usize) -> Option<&PatternDeltas> {
        self.patterns.get(pattern_idx)?.as_deref()
    }

    /// Number of trips carrying at least one adjustment.
    pub fn updated_trips(&self) -> usize {
        self.patterns
            .iter()
            .flatten()
            .map(|pattern| pattern.trips.len())
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Trip resolution
// ---------------------------------------------------------------------------

/// Maps GTFS `trip_id` to its position in the pattern index.
///
/// Built once per GTFS load and reused across refreshes: inverting ~500k trips
/// costs a fraction of a second, but doing it every 30 s would not be free.
pub struct TripLookup {
    by_trip_id: FxHashMap<String, (u32, u32)>,
}

impl TripLookup {
    pub fn build(data: &RaptorData) -> Self {
        let total: usize = data.patterns.iter().map(|p| p.trips.len()).sum();
        let mut by_trip_id = FxHashMap::with_capacity_and_hasher(total, Default::default());

        for (pattern_idx, pattern) in data.patterns.iter().enumerate() {
            for (trip_idx, trip) in pattern.trips.iter().enumerate() {
                by_trip_id.insert(trip.trip_id.clone(), (pattern_idx as u32, trip_idx as u32));
            }
        }
        Self { by_trip_id }
    }

    fn resolve(&self, trip_id: &str) -> Option<(usize, usize)> {
        self.by_trip_id
            .get(trip_id)
            .map(|&(pattern, trip)| (pattern as usize, trip as usize))
    }

    /// Number of trips resolvable from a feed identifier.
    pub fn trip_count(&self) -> usize {
        self.by_trip_id.len()
    }
}

// ---------------------------------------------------------------------------
// Building
// ---------------------------------------------------------------------------

/// Resolve every feed against the schedule into a single overlay.
///
/// Later feeds win on conflict, so the configuration order is also the
/// precedence order.
pub fn build_index<'a>(
    data: &RaptorData,
    lookup: &TripLookup,
    feeds: impl IntoIterator<Item = &'a RealtimeFeed>,
) -> RealtimeIndex {
    let mut index = RealtimeIndex {
        patterns: (0..data.patterns.len()).map(|_| None).collect(),
        stats: MatchStats::default(),
    };

    for feed in feeds {
        for update in &feed.trip_updates {
            apply_trip_update(data, lookup, update, &mut index);
        }
    }
    index
}

/// Resolve one trip update and merge it into the overlay.
fn apply_trip_update(
    data: &RaptorData,
    lookup: &TripLookup,
    update: &TripUpdate,
    index: &mut RealtimeIndex,
) {
    // Phase 1 routes over scheduled trips only. ADDED trips would mean
    // injecting stop sequences that no pattern holds.
    if !matches!(
        update.relationship,
        TripRelationship::Scheduled | TripRelationship::Canceled
    ) {
        index.stats.unsupported_trips += 1;
        return;
    }

    let Some(trip_id) = update.trip.trip_id.as_deref() else {
        index.stats.unmatched_trips += 1;
        return;
    };
    let Some((pattern_idx, trip_idx)) = lookup.resolve(trip_id) else {
        index.stats.unmatched_trips += 1;
        return;
    };
    index.stats.matched_trips += 1;

    let pattern = &data.patterns[pattern_idx];
    let delta = if update.relationship == TripRelationship::Canceled {
        index.stats.canceled_trips += 1;
        TripDelta {
            canceled: true,
            skips_calls: false,
            calls: Vec::new(),
        }
    } else {
        let calls = resolve_calls(data, pattern, trip_idx, update, &mut index.stats);
        TripDelta {
            canceled: false,
            skips_calls: calls.iter().any(|call| call.skipped),
            calls,
        }
    };

    let entry = index.patterns[pattern_idx].get_or_insert_with(Box::default);
    entry.max_abs_delta = entry.max_abs_delta.max(largest_offset(&delta));
    entry.trips.insert(trip_idx as u32, delta);
}

/// Largest absolute offset carried by a trip, in seconds.
fn largest_offset(delta: &TripDelta) -> u32 {
    delta
        .calls
        .iter()
        .map(|call| call.arrival.abs().max(call.departure.abs()) as u32)
        .max()
        .unwrap_or(0)
}

/// Turn a feed's calls into one dense offset per position in the pattern.
///
/// GTFS-Realtime only sends the calls it knows about; the offset of the last
/// one carries forward to every following call, and the trip-level delay seeds
/// the calls that precede the first update.
fn resolve_calls(
    data: &RaptorData,
    pattern: &Pattern,
    trip_idx: usize,
    update: &TripUpdate,
    stats: &mut MatchStats,
) -> Vec<CallDelta> {
    let trip = &pattern.trips[trip_idx];
    let seed = update.delay.unwrap_or(0);
    let mut calls = vec![
        CallDelta {
            arrival: seed,
            departure: seed,
            skipped: false,
        };
        pattern.stops.len()
    ];

    let midnight = update
        .trip
        .start_date
        .as_deref()
        .and_then(service_midnight_posix);

    let mut cursor = 0usize;
    let mut carried = CallDelta {
        arrival: seed,
        departure: seed,
        skipped: false,
    };

    for stop_update in &update.stop_updates {
        let Some(pos) = resolve_position(data, pattern, stop_update, &mut cursor, stats) else {
            stats.unmatched_calls += 1;
            continue;
        };
        stats.matched_calls += 1;

        let scheduled = trip.stop_times[pos];
        carried = CallDelta {
            arrival: offset_for(
                stop_update.arrival,
                scheduled.0,
                midnight,
                carried.arrival,
                stats,
            ),
            departure: offset_for(
                stop_update.departure,
                scheduled.1,
                midnight,
                carried.departure,
                stats,
            ),
            skipped: stop_update.relationship == StopRelationship::Skipped,
        };
        // Propagate forward: everything from here on inherits this offset until
        // the next update overrides it.
        for call in &mut calls[pos..] {
            *call = carried;
        }
        // `skipped` applies to this call only, not to the rest of the trip.
        if carried.skipped {
            for call in &mut calls[pos + 1..] {
                call.skipped = false;
            }
            carried.skipped = false;
        }
    }
    calls
}

/// Convert one prediction into an offset in seconds.
///
/// A feed may express a prediction as a delay, as an absolute time, or both.
/// Absolute times are resolved against the service date in the *server's* local
/// timezone, which is expected to match the network's.
fn offset_for(
    update: Option<TimeUpdate>,
    scheduled: u32,
    midnight: Option<i64>,
    carried: i32,
    stats: &mut MatchStats,
) -> i32 {
    let Some(update) = update else {
        return carried;
    };
    if let Some(delay) = update.delay {
        return delay;
    }
    let Some(time) = update.time else {
        return carried;
    };
    let Some(midnight) = midnight else {
        stats.unresolved_times += 1;
        return carried;
    };
    (time - (midnight + i64::from(scheduled))) as i32
}

/// POSIX timestamp of "noon minus twelve hours" on a `YYYYMMDD` service date.
///
/// GTFS counts times from that instant rather than from midnight so that a
/// service date keeps 24 h even when a DST change makes the day 23 or 25 hours
/// long.
fn service_midnight_posix(date: &str) -> Option<i64> {
    if date.len() != 8 {
        return None;
    }
    let year = date.get(0..4)?.parse::<i32>().ok()?;
    let month = date.get(4..6)?.parse::<u32>().ok()?;
    let day = date.get(6..8)?.parse::<u32>().ok()?;
    let noon = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(12, 0, 0)?;
    let local = Local.from_local_datetime(&noon).earliest()?;
    Some(local.timestamp() - 12 * 3600)
}

/// Locate a feed's call within the pattern.
///
/// `stop_id` is authoritative. The search starts at `cursor` and only moves
/// forward, so a stop served twice by a loop route resolves to the right visit.
/// `stop_sequence` is a last resort: `build_patterns` discards the original
/// values, so it only holds when the producer numbered calls from zero.
fn resolve_position(
    data: &RaptorData,
    pattern: &Pattern,
    stop_update: &super::model::StopTimeUpdate,
    cursor: &mut usize,
    stats: &mut MatchStats,
) -> Option<usize> {
    if let Some(stop_id) = stop_update.stop_id.as_deref()
        && let Some(&stop_idx) = data.stop_index.get(stop_id)
    {
        let found = pattern.stops[*cursor..]
            .iter()
            .position(|&s| s == stop_idx)
            .map(|offset| *cursor + offset);
        if let Some(pos) = found {
            *cursor = pos + 1;
            return Some(pos);
        }
    }

    let sequence = stop_update.stop_sequence? as usize;
    if sequence >= pattern.stops.len() || sequence < *cursor {
        return None;
    }
    stats.sequence_fallback_calls += 1;
    *cursor = sequence + 1;
    Some(sequence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transit::realtime::model::{StopTimeUpdate, TripRef};

    /// The shared three-stop fixture: S1 → S2 → S3, served by trips T1 and T2.
    fn test_data() -> RaptorData {
        crate::transit::raptor::test_support::build_test_data()
    }

    fn scheduled_update(trip_id: &str, stops: Vec<StopTimeUpdate>) -> TripUpdate {
        TripUpdate {
            trip: TripRef {
                trip_id: Some(trip_id.to_string()),
                ..Default::default()
            },
            relationship: TripRelationship::Scheduled,
            delay: None,
            stop_updates: stops,
            timestamp: None,
        }
    }

    fn call(stop_id: &str, delay: i32) -> StopTimeUpdate {
        StopTimeUpdate {
            stop_id: Some(stop_id.to_string()),
            stop_sequence: None,
            arrival: Some(TimeUpdate {
                delay: Some(delay),
                time: None,
            }),
            departure: Some(TimeUpdate {
                delay: Some(delay),
                time: None,
            }),
            relationship: StopRelationship::Scheduled,
        }
    }

    fn feed(updates: Vec<TripUpdate>) -> RealtimeFeed {
        RealtimeFeed {
            trip_updates: updates,
            timestamp: None,
        }
    }

    #[test]
    fn trip_lookup_covers_every_scheduled_trip() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let total: usize = data.patterns.iter().map(|p| p.trips.len()).sum();
        assert_eq!(lookup.trip_count(), total);
        assert!(lookup.resolve("T1").is_some());
        assert!(lookup.resolve("does-not-exist").is_none());
    }

    #[test]
    fn a_delay_shifts_the_call_it_targets_and_every_later_one() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let index = build_index(
            &data,
            &lookup,
            &[feed(vec![scheduled_update("T1", vec![call("S2", 120)])])],
        );

        let (pattern_idx, trip_idx) = lookup.resolve("T1").unwrap();
        let deltas = index.pattern(pattern_idx).unwrap();
        let delta = deltas.trip(trip_idx).unwrap();
        let trip = &data.patterns[pattern_idx].trips[trip_idx];

        // S1 is before the update: untouched.
        assert_eq!(
            delta.arrival(0, trip.stop_times[0].0),
            Some(trip.stop_times[0].0)
        );
        // S2 carries the delay, and S3 inherits it.
        assert_eq!(
            delta.arrival(1, trip.stop_times[1].0),
            Some(trip.stop_times[1].0 + 120)
        );
        assert_eq!(
            delta.arrival(2, trip.stop_times[2].0),
            Some(trip.stop_times[2].0 + 120)
        );
        assert_eq!(deltas.max_abs_delta(), 120);
    }

    #[test]
    fn a_negative_delay_moves_the_call_earlier_without_wrapping() {
        assert_eq!(shift(100, -30), 70);
        assert_eq!(shift(10, -30), 0, "must clamp rather than wrap around zero");
    }

    #[test]
    fn a_trip_level_delay_seeds_calls_with_no_prediction_of_their_own() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let mut update = scheduled_update("T1", vec![]);
        update.delay = Some(60);
        let index = build_index(&data, &lookup, &[feed(vec![update])]);

        let (pattern_idx, trip_idx) = lookup.resolve("T1").unwrap();
        let delta = index.pattern(pattern_idx).unwrap().trip(trip_idx).unwrap();
        let trip = &data.patterns[pattern_idx].trips[trip_idx];
        for pos in 0..data.patterns[pattern_idx].stops.len() {
            assert_eq!(
                delta.arrival(pos, trip.stop_times[pos].0),
                Some(trip.stop_times[pos].0 + 60)
            );
        }
    }

    #[test]
    fn a_cancelled_trip_is_flagged_and_counted() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let mut update = scheduled_update("T1", vec![]);
        update.relationship = TripRelationship::Canceled;
        let index = build_index(&data, &lookup, &[feed(vec![update])]);

        let (pattern_idx, trip_idx) = lookup.resolve("T1").unwrap();
        assert!(
            index
                .pattern(pattern_idx)
                .unwrap()
                .trip(trip_idx)
                .unwrap()
                .canceled
        );
        assert_eq!(index.stats.canceled_trips, 1);
    }

    #[test]
    fn a_skipped_call_reports_no_time_but_leaves_later_calls_alone() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let mut skipped = call("S2", 0);
        skipped.relationship = StopRelationship::Skipped;
        let index = build_index(
            &data,
            &lookup,
            &[feed(vec![scheduled_update("T1", vec![skipped])])],
        );

        let (pattern_idx, trip_idx) = lookup.resolve("T1").unwrap();
        let delta = index.pattern(pattern_idx).unwrap().trip(trip_idx).unwrap();
        assert_eq!(delta.arrival(1, 1000), None, "the skipped call is unusable");
        assert_eq!(delta.arrival(2, 1000), Some(1000), "later calls still run");
    }

    #[test]
    fn an_unknown_trip_id_is_counted_rather_than_silently_dropped() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let index = build_index(
            &data,
            &lookup,
            &[feed(vec![scheduled_update("NOPE", vec![call("S2", 60)])])],
        );
        assert_eq!(index.stats.unmatched_trips, 1);
        assert_eq!(index.stats.matched_trips, 0);
        assert_eq!(index.updated_trips(), 0);
    }

    #[test]
    fn a_call_at_a_stop_the_pattern_does_not_serve_is_counted() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let index = build_index(
            &data,
            &lookup,
            &[feed(vec![scheduled_update(
                "T1",
                vec![call("UNKNOWN_STOP", 60)],
            )])],
        );
        assert_eq!(index.stats.unmatched_calls, 1);
        assert_eq!(index.stats.matched_calls, 0);
    }

    #[test]
    fn added_trips_are_recorded_as_unsupported_in_phase_one() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let mut update = scheduled_update("T1", vec![]);
        update.relationship = TripRelationship::Added;
        let index = build_index(&data, &lookup, &[feed(vec![update])]);
        assert_eq!(index.stats.unsupported_trips, 1);
        assert_eq!(index.updated_trips(), 0);
    }

    #[test]
    fn patterns_without_updates_stay_empty() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let index = build_index(&data, &lookup, &[]);
        assert!((0..data.patterns.len()).all(|p| index.pattern(p).is_none()));
    }

    #[test]
    fn an_absolute_time_becomes_an_offset_against_the_service_date() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let (pattern_idx, trip_idx) = lookup.resolve("T1").unwrap();
        let scheduled = data.patterns[pattern_idx].trips[trip_idx].stop_times[1].0;
        let midnight = service_midnight_posix("20260406").unwrap();

        let mut update = scheduled_update(
            "T1",
            vec![StopTimeUpdate {
                stop_id: Some("S2".to_string()),
                stop_sequence: None,
                arrival: Some(TimeUpdate {
                    delay: None,
                    time: Some(midnight + i64::from(scheduled) + 90),
                }),
                departure: None,
                relationship: StopRelationship::Scheduled,
            }],
        );
        update.trip.start_date = Some("20260406".to_string());

        let index = build_index(&data, &lookup, &[feed(vec![update])]);
        let delta = index.pattern(pattern_idx).unwrap().trip(trip_idx).unwrap();
        assert_eq!(delta.arrival(1, scheduled), Some(scheduled + 90));
        assert_eq!(index.stats.unresolved_times, 0);
    }

    #[test]
    fn an_absolute_time_without_a_service_date_is_counted_as_unresolved() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let update = scheduled_update(
            "T1",
            vec![StopTimeUpdate {
                stop_id: Some("S2".to_string()),
                stop_sequence: None,
                arrival: Some(TimeUpdate {
                    delay: None,
                    time: Some(1_700_000_000),
                }),
                departure: None,
                relationship: StopRelationship::Scheduled,
            }],
        );
        let index = build_index(&data, &lookup, &[feed(vec![update])]);
        assert_eq!(index.stats.unresolved_times, 1);
    }

    #[test]
    fn service_midnight_rejects_malformed_dates() {
        assert!(service_midnight_posix("2026-04-06").is_none());
        assert!(service_midnight_posix("20260430x").is_none());
        assert!(service_midnight_posix("20261332").is_none());
        assert!(service_midnight_posix("").is_none());
    }

    #[test]
    fn the_last_feed_wins_when_two_describe_the_same_trip() {
        let data = test_data();
        let lookup = TripLookup::build(&data);
        let index = build_index(
            &data,
            &lookup,
            &[
                feed(vec![scheduled_update("T1", vec![call("S2", 60)])]),
                feed(vec![scheduled_update("T1", vec![call("S2", 300)])]),
            ],
        );
        let (pattern_idx, trip_idx) = lookup.resolve("T1").unwrap();
        let delta = index.pattern(pattern_idx).unwrap().trip(trip_idx).unwrap();
        assert_eq!(delta.arrival(1, 1000), Some(1300));
    }
}
