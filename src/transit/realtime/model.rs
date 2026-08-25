//! Normalized real-time model, shared by every connector.
//!
//! The vocabulary follows GTFS-Realtime because it is the most expressive of
//! the two formats we target: SIRI concepts map onto it cleanly (an
//! `EstimatedVehicleJourney` becomes a [`TripUpdate`], an
//! `EstimatedCall` becomes a [`StopTimeUpdate`]), whereas the reverse mapping
//! loses information. Connectors translate into these types; everything
//! downstream — indexing, RAPTOR, the API — knows only this model.

/// How a real-time update relates to the scheduled trip it references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TripRelationship {
    /// The trip runs as scheduled, possibly with the delays listed in
    /// [`TripUpdate::stop_updates`].
    #[default]
    Scheduled,
    /// The trip is cancelled: it must not be used by the router.
    Canceled,
    /// The trip is not in the GTFS schedule at all. Phase 1 records these but
    /// the router ignores them — injecting new trips into a pattern is a
    /// separate piece of work.
    Added,
    /// Running without a schedule (frequency-based). Ignored by the router.
    Unscheduled,
    /// A copy of an existing trip running at a different time. Ignored.
    Duplicated,
}

/// How a real-time update relates to a scheduled call at a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StopRelationship {
    /// The vehicle calls at this stop, at the time given by the update.
    #[default]
    Scheduled,
    /// The vehicle does not call at this stop on this trip.
    Skipped,
    /// No real-time information for this stop; fall back on the schedule.
    NoData,
}

/// Identifies the scheduled trip an update applies to.
///
/// `trip_id` is the reliable key when it is present and uses the same
/// namespace as the GTFS feed. `route_id` and `start_date` are kept for
/// connectors (notably SIRI) whose journey references need remapping before
/// they can be resolved.
#[derive(Debug, Clone, Default)]
pub struct TripRef {
    pub trip_id: Option<String>,
    pub route_id: Option<String>,
    /// Service date the trip started on, `YYYYMMDD`.
    pub start_date: Option<String>,
    /// Scheduled start time, `HH:MM:SS`.
    pub start_time: Option<String>,
}

/// A predicted time for one call, expressed as a delay, an absolute time, or
/// both.
///
/// GTFS-Realtime allows either form; a feed that sends only `time` still
/// yields a delay once we know the scheduled time, which is why resolution is
/// deferred to the indexing step.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimeUpdate {
    /// Seconds late (positive) or early (negative) relative to the schedule.
    pub delay: Option<i32>,
    /// Absolute POSIX timestamp of the predicted event.
    pub time: Option<i64>,
}

impl TimeUpdate {
    /// Whether this update carries any usable prediction.
    pub fn is_empty(&self) -> bool {
        self.delay.is_none() && self.time.is_none()
    }
}

/// A real-time prediction for one call of one trip.
#[derive(Debug, Clone, Default)]
pub struct StopTimeUpdate {
    /// GTFS `stop_id`, the primary key used to locate the call in a pattern.
    pub stop_id: Option<String>,
    /// GTFS `stop_times.stop_sequence`, used only as a fallback.
    pub stop_sequence: Option<u32>,
    pub arrival: Option<TimeUpdate>,
    pub departure: Option<TimeUpdate>,
    pub relationship: StopRelationship,
}

/// Real-time information for a single trip.
#[derive(Debug, Clone, Default)]
pub struct TripUpdate {
    pub trip: TripRef,
    pub relationship: TripRelationship,
    /// Trip-level delay applied to every call without its own prediction.
    pub delay: Option<i32>,
    /// Per-call predictions, in the order sent by the feed.
    pub stop_updates: Vec<StopTimeUpdate>,
    /// When the producer last observed this trip (POSIX seconds).
    pub timestamp: Option<u64>,
}

/// One decoded snapshot from one source.
#[derive(Debug, Clone, Default)]
pub struct RealtimeFeed {
    pub trip_updates: Vec<TripUpdate>,
    /// Producer timestamp for the whole snapshot (POSIX seconds).
    pub timestamp: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_update_is_empty_without_any_prediction() {
        assert!(TimeUpdate::default().is_empty());
        assert!(
            !TimeUpdate {
                delay: Some(0),
                time: None,
            }
            .is_empty()
        );
        assert!(
            !TimeUpdate {
                delay: None,
                time: Some(1_700_000_000),
            }
            .is_empty()
        );
    }

    #[test]
    fn defaults_assume_the_schedule_holds() {
        assert_eq!(TripRelationship::default(), TripRelationship::Scheduled);
        assert_eq!(StopRelationship::default(), StopRelationship::Scheduled);
    }
}
