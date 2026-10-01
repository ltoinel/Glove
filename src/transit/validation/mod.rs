//! GTFS feed validation: rules run over the raw feed, before indexing.
//!
//! Each rule inspects one concern and yields zero or more findings; the
//! orchestrator stamps them with the rule's identity and builds a
//! [`ValidationReport`] that lists every rule — passed ones included — so an
//! exported report states what was checked, not only what failed.
//!
//! - [`integrity`] — references between files, duplicate IDs, stop types
//! - [`calendar`]  — service days, feed validity period
//! - [`schedule`]  — per-trip checks: times, sequences, speeds, restrictions
//! - [`network`]   — stops, routes, transfers and pathways

mod calendar;
mod integrity;
mod network;
mod schedule;

use chrono::NaiveDate;
use rustc_hash::FxHashMap;
use serde::Serialize;
use utoipa::ToSchema;

use super::gtfs::{GtfsData, Stop, StopTime};

/// Sample IDs kept per issue. Enough for a downloadable report to be
/// actionable, small enough to keep the response light.
pub const MAX_SAMPLES: usize = 20;

/// Severity level for a validation issue.
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Category of a validation issue.
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Loading,
    ReferentialIntegrity,
    Calendar,
    Schedule,
    Coordinates,
    Transfers,
    Pathways,
    Display,
}

/// A single validation issue found in the GTFS data.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ValidationIssue {
    /// Identifier of the rule that raised it (stable, e.g. `time_travel`).
    pub rule: String,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    /// Number of affected entities.
    pub count: usize,
    /// Sample IDs illustrating the issue (at most [`MAX_SAMPLES`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<String>,
}

/// Outcome of one rule: passed, or the worst severity it raised.
#[derive(Debug, Clone, Copy, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleStatus {
    Passed,
    Error,
    Warning,
    Info,
}

/// One rule of the catalogue and how it went.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuleOutcome {
    pub id: String,
    pub title: String,
    pub category: Category,
    pub status: RuleStatus,
}

/// Summary counts for each severity level.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct ValidationSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub total_checks: usize,
}

/// Size and period of the validated feed, for the report header.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FeedOverview {
    pub agencies: usize,
    pub routes: usize,
    pub stops: usize,
    pub trips: usize,
    pub stop_times: usize,
    pub calendars: usize,
    pub calendar_dates: usize,
    pub transfers: usize,
    pub pathways: usize,
    /// First service day (YYYY-MM-DD), if any service is defined.
    pub service_start: Option<String>,
    /// Last service day (YYYY-MM-DD), if any service is defined.
    pub service_end: Option<String>,
}

/// Everything `GET /api/gtfs/validate` returns.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ValidationReport {
    /// When the validation ran (RFC 3339).
    pub generated_at: String,
    /// The day "today" rules were evaluated against (YYYY-MM-DD).
    pub reference_date: String,
    pub feed: FeedOverview,
    pub summary: ValidationSummary,
    /// Every rule run, in catalogue order.
    pub rules: Vec<RuleOutcome>,
    /// Issues found, errors first.
    pub issues: Vec<ValidationIssue>,
}

/// Geographic area the feed is expected to cover.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub sw_lat: f64,
    pub sw_lon: f64,
    pub ne_lat: f64,
    pub ne_lon: f64,
}

impl Bounds {
    fn contains(&self, lat: f64, lon: f64) -> bool {
        (self.sw_lat..=self.ne_lat).contains(&lat) && (self.sw_lon..=self.ne_lon).contains(&lon)
    }
}

/// Inputs from outside the feed itself.
pub struct ValidationInput {
    /// Day the calendar rules consider "today".
    pub today: NaiveDate,
    /// Expected coverage area; `None` skips the out-of-area rule.
    pub bounds: Option<Bounds>,
    /// `location_type` per stop ID. Empty skips the rules that need it
    /// (it is loaded separately from [`GtfsData`], see
    /// [`super::gtfs::load_location_types`]).
    pub location_types: FxHashMap<String, u8>,
}

/// A trip's calls, ordered by `stop_sequence`.
struct TripCalls<'a> {
    trip_id: &'a str,
    calls: Vec<&'a StopTime>,
}

/// Shared, precomputed views over the feed. Built once, so the per-trip
/// rules do not each regroup ~11 M stop times. Every view is sorted by ID so
/// the samples a rule keeps are the same from one run to the next.
struct Ctx<'a> {
    gtfs: &'a GtfsData,
    input: &'a ValidationInput,
    trips: Vec<TripCalls<'a>>,
    stops: Vec<&'a Stop>,
}

impl<'a> Ctx<'a> {
    fn new(gtfs: &'a GtfsData, input: &'a ValidationInput) -> Self {
        let mut by_trip: FxHashMap<&str, Vec<&StopTime>> = FxHashMap::default();
        for st in &gtfs.stop_times {
            by_trip.entry(&st.trip_id).or_default().push(st);
        }
        let mut trips: Vec<TripCalls> = by_trip
            .into_iter()
            .map(|(trip_id, mut calls)| {
                calls.sort_by_key(|st| st.stop_sequence);
                TripCalls { trip_id, calls }
            })
            .collect();
        trips.sort_unstable_by_key(|t| t.trip_id);

        let mut stops: Vec<&Stop> = gtfs.stops.values().collect();
        stops.sort_unstable_by(|a, b| a.stop_id.cmp(&b.stop_id));

        Self {
            gtfs,
            input,
            trips,
            stops,
        }
    }

    /// `location_type` of a stop, when the column was loaded.
    fn location_type(&self, stop_id: &str) -> Option<u8> {
        self.input.location_types.get(stop_id).copied()
    }
}

/// What a rule reports; the orchestrator adds the rule's identity.
struct Finding {
    severity: Severity,
    message: String,
    count: usize,
    samples: Vec<String>,
}

/// Counts offending entities and keeps the first [`MAX_SAMPLES`] of them.
/// Samples are formatted lazily: most of the ~11 M stop times never become
/// strings.
#[derive(Default)]
struct Collector {
    count: usize,
    samples: Vec<String>,
}

impl Collector {
    fn push(&mut self, sample: impl FnOnce() -> String) {
        self.count += 1;
        if self.samples.len() < MAX_SAMPLES {
            self.samples.push(sample());
        }
    }

    /// Turn the collected entities into a finding, or nothing if none.
    fn finding(self, severity: Severity, message: impl Into<String>) -> Option<Finding> {
        (self.count > 0).then(|| Finding {
            severity,
            message: message.into(),
            count: self.count,
            samples: self.samples,
        })
    }
}

/// One entry of the rule catalogue.
#[derive(Clone, Copy)]
struct Rule {
    id: &'static str,
    title: &'static str,
    category: Category,
    run: fn(&Ctx) -> Vec<Finding>,
}

/// The catalogue, in report order.
fn rules() -> Vec<Rule> {
    [
        integrity::RULES,
        calendar::RULES,
        schedule::RULES,
        network::RULES,
    ]
    .concat()
}

/// Run every rule against `gtfs` and assemble the report.
pub fn validate(gtfs: &GtfsData, input: &ValidationInput) -> ValidationReport {
    let ctx = Ctx::new(gtfs, input);
    let mut outcomes = Vec::new();
    let mut issues = Vec::new();

    for rule in rules() {
        let findings = (rule.run)(&ctx);
        let worst = findings.iter().map(|f| f.severity).min();
        outcomes.push(RuleOutcome {
            id: rule.id.into(),
            title: rule.title.into(),
            category: rule.category,
            status: match worst {
                None => RuleStatus::Passed,
                Some(Severity::Error) => RuleStatus::Error,
                Some(Severity::Warning) => RuleStatus::Warning,
                Some(Severity::Info) => RuleStatus::Info,
            },
        });
        issues.extend(findings.into_iter().map(|f| ValidationIssue {
            rule: rule.id.into(),
            severity: f.severity,
            category: rule.category,
            message: f.message,
            count: f.count,
            samples: f.samples,
        }));
    }
    // Stable: within a severity, issues keep catalogue order.
    issues.sort_by_key(|i| i.severity);

    let count_of = |severity| issues.iter().filter(|i| i.severity == severity).count();
    let summary = ValidationSummary {
        errors: count_of(Severity::Error),
        warnings: count_of(Severity::Warning),
        infos: count_of(Severity::Info),
        total_checks: outcomes.len(),
    };

    ValidationReport {
        generated_at: chrono::Local::now().to_rfc3339(),
        reference_date: input.today.format("%Y-%m-%d").to_string(),
        feed: feed_overview(gtfs),
        summary,
        rules: outcomes,
        issues,
    }
}

fn feed_overview(gtfs: &GtfsData) -> FeedOverview {
    let period = calendar::service_period(gtfs);
    let iso = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    FeedOverview {
        agencies: gtfs.agencies.len(),
        routes: gtfs.routes.len(),
        stops: gtfs.stops.len(),
        trips: gtfs.trips.len(),
        stop_times: gtfs.stop_times.len(),
        calendars: gtfs.calendars.len(),
        calendar_dates: gtfs.calendar_dates.len(),
        transfers: gtfs.transfers.len(),
        pathways: gtfs.pathways.len(),
        service_start: period.map(|(start, _)| iso(start)),
        service_end: period.map(|(_, end)| iso(end)),
    }
}

#[cfg(test)]
mod tests;
