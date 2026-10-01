use chrono::NaiveDate;
use rustc_hash::FxHashMap;

use super::*;
use crate::transit::gtfs::{
    Agency, Calendar, CalendarDate, GtfsData, Pathway, Route, Stop, StopTime, Transfer, Trip,
};

/// Monday 2026-06-15: inside the fixture's service period.
fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()
}

fn stop(id: &str, lat: f64, lon: f64, parent: &str) -> Stop {
    Stop {
        stop_id: id.into(),
        stop_name: format!("Stop {id}"),
        stop_lat: lat,
        stop_lon: lon,
        parent_station: parent.into(),
        wheelchair_boarding: 0,
    }
}

fn call(trip: &str, seq: u32, stop: &str, arr: &str, dep: &str) -> StopTime {
    StopTime {
        trip_id: trip.into(),
        stop_sequence: seq,
        stop_id: stop.into(),
        arrival_time: arr.into(),
        departure_time: dep.into(),
        ..Default::default()
    }
}

fn agency(id: &str) -> Agency {
    serde_json::from_value(serde_json::json!({
        "agency_id": id, "agency_name": id, "agency_url": "https://example.org",
        "agency_timezone": "Europe/Paris"
    }))
    .unwrap()
}

/// A metro line (S1 → S2 → S3, ~1 km apart) running every day of 2026,
/// with a station grouping S1 and S1b joined by a transfer.
fn clean_feed() -> GtfsData {
    let stops = [
        stop("S1", 48.8500, 2.3500, "ST1"),
        stop("S1b", 48.8501, 2.3502, "ST1"),
        stop("S2", 48.8590, 2.3500, ""),
        stop("S3", 48.8680, 2.3500, ""),
        stop("ST1", 48.8500, 2.3501, ""),
    ];
    let route = Route {
        route_id: "R1".into(),
        agency_id: "A1".into(),
        route_short_name: "1".into(),
        route_long_name: "Line 1".into(),
        route_type: 1,
        route_color: "FFCD00".into(),
        route_text_color: "000000".into(),
    };
    let trip = Trip {
        route_id: "R1".into(),
        service_id: "WEEK".into(),
        trip_id: "T1".into(),
        trip_headsign: "S3".into(),
        wheelchair_accessible: 0,
    };
    let calendar = Calendar {
        service_id: "WEEK".into(),
        monday: 1,
        tuesday: 1,
        wednesday: 1,
        thursday: 1,
        friday: 1,
        saturday: 1,
        sunday: 1,
        start_date: "20260101".into(),
        end_date: "20261231".into(),
    };
    GtfsData {
        agencies: vec![agency("A1")],
        routes: FxHashMap::from_iter([("R1".into(), route)]),
        stops: stops.into_iter().map(|s| (s.stop_id.clone(), s)).collect(),
        trips: FxHashMap::from_iter([("T1".into(), trip)]),
        stop_times: vec![
            call("T1", 1, "S1", "08:00:00", "08:00:30"),
            call("T1", 2, "S2", "08:02:00", "08:02:30"),
            call("T1", 3, "S3", "08:04:00", "08:04:00"),
        ],
        calendars: FxHashMap::from_iter([("WEEK".into(), calendar)]),
        transfers: vec![Transfer {
            from_stop_id: "S1".into(),
            to_stop_id: "S1b".into(),
            min_transfer_time: Some(120),
        }],
        ..Default::default()
    }
}

fn input() -> ValidationInput {
    ValidationInput {
        today: today(),
        bounds: Some(Bounds {
            sw_lat: 48.1,
            sw_lon: 1.4,
            ne_lat: 49.3,
            ne_lon: 3.6,
        }),
        location_types: FxHashMap::from_iter([
            ("S1".into(), 0),
            ("S1b".into(), 0),
            ("S2".into(), 0),
            ("S3".into(), 0),
            ("ST1".into(), 1),
        ]),
    }
}

fn issue<'r>(report: &'r ValidationReport, rule: &str) -> Option<&'r ValidationIssue> {
    report.issues.iter().find(|i| i.rule == rule)
}

fn status(report: &ValidationReport, rule: &str) -> RuleStatus {
    report
        .rules
        .iter()
        .find(|r| r.id == rule)
        .unwrap_or_else(|| panic!("rule {rule} not in catalogue"))
        .status
}

#[test]
fn clean_feed_has_no_error_nor_warning() {
    let report = validate(&clean_feed(), &input());
    let problems: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.severity != Severity::Info)
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(report.summary.total_checks, report.rules.len());
    assert_eq!(report.reference_date, "2026-06-15");
    assert_eq!(report.feed.service_start.as_deref(), Some("2026-01-01"));
    assert_eq!(report.feed.service_end.as_deref(), Some("2026-12-31"));
}

#[test]
fn rule_ids_are_unique() {
    let ids: Vec<_> = rules().iter().map(|r| r.id).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
}

#[test]
fn every_issue_names_a_rule_of_the_catalogue() {
    let mut gtfs = clean_feed();
    gtfs.stop_times.push(call("GHOST", 1, "NOWHERE", "x", "y"));
    let report = validate(&gtfs, &input());
    for i in &report.issues {
        assert!(report.rules.iter().any(|r| r.id == i.rule), "{}", i.rule);
    }
    assert_eq!(status(&report, "stop_times_trip_ref"), RuleStatus::Error);
    assert_eq!(status(&report, "stop_times_stop_ref"), RuleStatus::Error);
}

#[test]
fn issues_are_sorted_errors_first() {
    let mut gtfs = clean_feed();
    gtfs.stops.get_mut("S2").unwrap().stop_name.clear(); // warning
    gtfs.stop_times
        .push(call("GHOST", 1, "S1", "08:00:00", "08:00:00")); // error
    let report = validate(&gtfs, &input());
    let severities: Vec<_> = report.issues.iter().map(|i| i.severity).collect();
    let mut sorted = severities.clone();
    sorted.sort();
    assert_eq!(severities, sorted);
}

// --- loading ----------------------------------------------------------------

#[test]
fn load_report_surfaces_duplicates_and_malformed_rows() {
    let mut gtfs = clean_feed();
    gtfs.load_report.duplicate_ids = vec![("stops.txt", vec!["S1".into(), "S2".into()])];
    gtfs.load_report.malformed_rows = vec![("stop_times.txt", 3)];
    let report = validate(&gtfs, &input());
    let dup = issue(&report, "duplicate_ids").unwrap();
    assert_eq!((dup.severity, dup.count), (Severity::Error, 2));
    assert_eq!(dup.samples[0], "stops.txt: S1");
    let bad = issue(&report, "malformed_rows").unwrap();
    assert_eq!((bad.severity, bad.count), (Severity::Warning, 3));
}

// --- integrity --------------------------------------------------------------

#[test]
fn unknown_agency_is_an_error_missing_one_a_warning_with_several_agencies() {
    let mut gtfs = clean_feed();
    gtfs.routes.get_mut("R1").unwrap().agency_id = "NOPE".into();
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "routes_agency_ref"), RuleStatus::Error);

    gtfs.routes.get_mut("R1").unwrap().agency_id.clear();
    assert_eq!(
        status(&validate(&gtfs, &input()), "routes_agency_ref"),
        RuleStatus::Passed,
        "empty agency_id is fine with a single agency"
    );
    gtfs.agencies.push(agency("A2"));
    assert_eq!(
        status(&validate(&gtfs, &input()), "routes_agency_ref"),
        RuleStatus::Warning
    );
}

#[test]
fn stop_times_at_a_station_are_rejected() {
    let mut gtfs = clean_feed();
    gtfs.stop_times[1].stop_id = "ST1".into();
    let report = validate(&gtfs, &input());
    let i = issue(&report, "stop_times_location_type").unwrap();
    assert_eq!(i.samples, vec!["ST1 (location_type=1)"]);
}

#[test]
fn platform_whose_parent_is_not_a_station_is_flagged() {
    let mut gtfs = clean_feed();
    gtfs.stops.get_mut("S1b").unwrap().parent_station = "S1".into();
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "parent_station_type"), RuleStatus::Warning);
}

#[test]
fn location_type_rules_are_skipped_without_the_column() {
    let mut gtfs = clean_feed();
    gtfs.stop_times[1].stop_id = "ST1".into();
    let mut no_types = input();
    no_types.location_types.clear();
    let report = validate(&gtfs, &no_types);
    assert_eq!(
        status(&report, "stop_times_location_type"),
        RuleStatus::Passed
    );
    assert_eq!(status(&report, "stops_unused"), RuleStatus::Passed);
}

// --- calendar ---------------------------------------------------------------

#[test]
fn expired_feed_is_an_error() {
    let mut late = input();
    late.today = NaiveDate::from_ymd_opt(2027, 1, 5).unwrap();
    let report = validate(&clean_feed(), &late);
    let i = issue(&report, "feed_validity").unwrap();
    assert_eq!(i.severity, Severity::Error);
    assert!(i.message.contains("expired on 2026-12-31"), "{}", i.message);
    assert_eq!(status(&report, "services_today"), RuleStatus::Error);
}

#[test]
fn feed_expiring_within_a_week_is_a_warning() {
    let mut soon = input();
    soon.today = NaiveDate::from_ymd_opt(2026, 12, 28).unwrap();
    let i = validate(&clean_feed(), &soon);
    let i = issue(&i, "feed_validity").unwrap();
    assert_eq!(i.severity, Severity::Warning);
    assert!(i.message.contains("3 day(s)"), "{}", i.message);
}

#[test]
fn services_today_honors_weekdays_and_exceptions() {
    let mut gtfs = clean_feed();
    // Weekends only: nothing runs on Monday 2026-06-15...
    let cal = gtfs.calendars.get_mut("WEEK").unwrap();
    (
        cal.monday,
        cal.tuesday,
        cal.wednesday,
        cal.thursday,
        cal.friday,
    ) = (0, 0, 0, 0, 0);
    assert_eq!(
        status(&validate(&gtfs, &input()), "services_today"),
        RuleStatus::Error
    );
    // ...unless a calendar_dates exception adds it.
    gtfs.calendar_dates.push(CalendarDate {
        service_id: "WEEK".into(),
        date: "20260615".into(),
        exception_type: 1,
    });
    assert_eq!(
        status(&validate(&gtfs, &input()), "services_today"),
        RuleStatus::Info
    );
}

#[test]
fn service_with_no_weekday_and_no_added_date_never_runs() {
    let mut gtfs = clean_feed();
    let mut never = gtfs.calendars["WEEK"].clone_for_test("NEVER");
    (
        never.monday,
        never.tuesday,
        never.wednesday,
        never.thursday,
        never.friday,
        never.saturday,
        never.sunday,
    ) = (0, 0, 0, 0, 0, 0, 0);
    gtfs.calendars.insert("NEVER".into(), never);
    let report = validate(&gtfs, &input());
    assert_eq!(
        issue(&report, "services_never_run").unwrap().samples,
        vec!["NEVER"]
    );
    assert_eq!(
        issue(&report, "services_unused").unwrap().samples,
        vec!["NEVER"]
    );
}

// --- schedule ---------------------------------------------------------------

#[test]
fn flex_calls_with_empty_times_are_not_errors() {
    let mut gtfs = clean_feed();
    gtfs.stop_times.push(StopTime {
        has_pickup_window: true,
        ..call("T1", 4, "S3", "", "")
    });
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "unparseable_times"), RuleStatus::Passed);
    assert_eq!(status(&report, "flex_calls"), RuleStatus::Info);
}

#[test]
fn empty_times_on_an_exact_timepoint_are_errors() {
    let mut gtfs = clean_feed();
    gtfs.stop_times[1].arrival_time.clear();
    gtfs.stop_times[1].departure_time.clear();
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "unparseable_times"), RuleStatus::Error);
}

#[test]
fn times_going_backwards_are_caught() {
    let mut gtfs = clean_feed();
    gtfs.stop_times[1].arrival_time = "07:59:00".into(); // before leaving S1 at 08:00:30
    let report = validate(&gtfs, &input());
    let i = issue(&report, "time_travel").unwrap();
    assert_eq!(i.severity, Severity::Error);
    assert!(i.samples[0].contains("seq 1→2"), "{}", i.samples[0]);
}

#[test]
fn departure_before_arrival_is_caught() {
    let mut gtfs = clean_feed();
    gtfs.stop_times[1].departure_time = "08:01:00".into(); // arrives 08:02:00
    let report = validate(&gtfs, &input());
    assert!(issue(&report, "time_travel").unwrap().samples[0].contains("departs"));
}

#[test]
fn duplicate_sequence_is_caught() {
    let mut gtfs = clean_feed();
    gtfs.stop_times[2].stop_sequence = 2;
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "duplicate_sequences"), RuleStatus::Error);
}

#[test]
fn metro_teleporting_ten_km_in_a_minute_is_implausible() {
    let mut gtfs = clean_feed();
    gtfs.stops.get_mut("S3").unwrap().stop_lat = 48.95; // ~9 km from S2, 90 s later
    let report = validate(&gtfs, &input());
    let i = issue(&report, "implausible_speed").unwrap();
    assert_eq!(i.count, 1);
    assert!(i.samples[0].contains("seq 2→3"), "{}", i.samples[0]);
}

#[test]
fn minute_rounding_alone_is_not_a_teleport() {
    let mut gtfs = clean_feed();
    // 1 km with identical departure and arrival minutes: 60 km/h once the
    // rounding allowance is added, fine for a metro.
    gtfs.stop_times[1].arrival_time = "08:00:30".into();
    gtfs.stop_times[1].departure_time = "08:00:30".into();
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "implausible_speed"), RuleStatus::Passed);
}

#[test]
fn trip_without_calls_and_single_stop_trip_are_flagged() {
    let mut gtfs = clean_feed();
    for id in ["EMPTY", "ONE"] {
        let mut trip = gtfs.trips["T1"].clone_for_test(id);
        trip.trip_id = id.into();
        gtfs.trips.insert(id.into(), trip);
    }
    gtfs.stop_times
        .push(call("ONE", 1, "S1", "09:00:00", "09:00:00"));
    let report = validate(&gtfs, &input());
    assert_eq!(
        issue(&report, "trips_without_calls").unwrap().samples,
        vec!["EMPTY"]
    );
    assert!(issue(&report, "short_trips").unwrap().samples[0].starts_with("ONE"));
}

#[test]
fn only_intermediate_boarding_restrictions_are_reported() {
    let mut gtfs = clean_feed();
    gtfs.stop_times[0].drop_off_type = 1; // first stop: normal
    gtfs.stop_times[2].pickup_type = 1; // last stop: normal
    assert_eq!(
        status(&validate(&gtfs, &input()), "boarding_restrictions"),
        RuleStatus::Passed
    );
    gtfs.stop_times[1].pickup_type = 1; // intermediate: reported
    let report = validate(&gtfs, &input());
    assert!(issue(&report, "boarding_restrictions").unwrap().samples[0].contains("no boarding"));
}

// --- network ----------------------------------------------------------------

#[test]
fn stop_outside_the_configured_area_is_flagged() {
    let mut gtfs = clean_feed();
    // Swapped lat/lon: valid coordinates, wrong place.
    let s3 = gtfs.stops.get_mut("S3").unwrap();
    (s3.stop_lat, s3.stop_lon) = (2.35, 48.868);
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "stops_outside_area"), RuleStatus::Warning);
}

#[test]
fn unused_platform_and_route_without_trips_are_reported() {
    let mut gtfs = clean_feed();
    gtfs.stops.insert("S4".into(), stop("S4", 48.86, 2.36, ""));
    let mut inp = input();
    inp.location_types.insert("S4".into(), 0);
    let mut r2 = gtfs.routes["R1"].clone_for_test("R2");
    r2.route_type = 42;
    gtfs.routes.insert("R2".into(), r2);
    let report = validate(&gtfs, &inp);
    let unused = &issue(&report, "stops_unused").unwrap().samples;
    assert!(unused.iter().any(|s| s.starts_with("S4")), "{unused:?}");
    assert!(issue(&report, "routes_without_trips").unwrap().samples[0].starts_with("R2"));
    assert_eq!(
        issue(&report, "route_type_unknown").unwrap().samples,
        vec!["R2 route_type=42"]
    );
}

#[test]
fn transfer_too_long_to_walk_is_flagged() {
    let mut gtfs = clean_feed();
    gtfs.transfers.push(Transfer {
        from_stop_id: "S1".into(),
        to_stop_id: "S3".into(), // ~2 km
        min_transfer_time: Some(120),
    });
    let report = validate(&gtfs, &input());
    let i = issue(&report, "transfer_walk_speed").unwrap();
    assert_eq!(i.count, 1);
    assert!(i.samples[0].starts_with("S1 → S3"), "{}", i.samples[0]);
}

#[test]
fn unknown_pathway_mode_is_an_error() {
    let mut gtfs = clean_feed();
    gtfs.pathways.push(Pathway {
        from_stop_id: "S1".into(),
        to_stop_id: "S1b".into(),
        pathway_mode: 9,
        is_bidirectional: 1,
        traversal_time: Some(30),
    });
    let report = validate(&gtfs, &input());
    assert_eq!(status(&report, "pathway_mode"), RuleStatus::Error);
    assert_eq!(status(&report, "pathway_times"), RuleStatus::Passed);
}

#[test]
fn samples_are_capped() {
    let mut gtfs = clean_feed();
    for n in 0..(MAX_SAMPLES + 5) {
        gtfs.stop_times.push(call(
            &format!("GHOST{n:03}"),
            1,
            "S1",
            "08:00:00",
            "08:00:00",
        ));
    }
    let report = validate(&gtfs, &input());
    let i = issue(&report, "stop_times_trip_ref").unwrap();
    assert_eq!(i.count, MAX_SAMPLES + 5);
    assert_eq!(i.samples.len(), MAX_SAMPLES);
    assert_eq!(i.samples[0], "GHOST000", "samples follow ID order");
}

/// The GTFS model types derive no `Clone` (they are loaded once, moved into
/// the index); tests need copies under another ID.
trait CloneForTest {
    fn clone_for_test(&self, id: &str) -> Self;
}

impl CloneForTest for Calendar {
    fn clone_for_test(&self, id: &str) -> Self {
        Calendar {
            service_id: id.into(),
            start_date: self.start_date.clone(),
            end_date: self.end_date.clone(),
            ..*self
        }
    }
}

impl CloneForTest for Trip {
    fn clone_for_test(&self, id: &str) -> Self {
        Trip {
            trip_id: id.into(),
            route_id: self.route_id.clone(),
            service_id: self.service_id.clone(),
            trip_headsign: self.trip_headsign.clone(),
            wheelchair_accessible: self.wheelchair_accessible,
        }
    }
}

impl CloneForTest for Route {
    fn clone_for_test(&self, id: &str) -> Self {
        Route {
            route_id: id.into(),
            agency_id: self.agency_id.clone(),
            route_short_name: id.into(),
            route_long_name: self.route_long_name.clone(),
            route_type: self.route_type,
            route_color: self.route_color.clone(),
            route_text_color: self.route_text_color.clone(),
        }
    }
}
