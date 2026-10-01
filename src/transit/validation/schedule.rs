//! Per-trip checks over `stop_times`: times that parse and move forward,
//! sequences, speeds between consecutive stops, and boarding restrictions.

use rustc_hash::FxHashSet;

use super::{Category, Collector, Ctx, Finding, Rule, Severity, TripCalls};
use crate::shared::util::haversine_meters;
use crate::transit::gtfs::{self, StopTime};

/// GTFS times are usually rounded to the minute: two stops shown one minute
/// apart may really be two minutes apart. Added to every interval before
/// computing a speed, so rounding alone never looks like a teleport.
const MINUTE_ROUNDING_S: f64 = 60.0;

/// `pickup_type` / `drop_off_type` value: no boarding / no alighting.
const NOT_AVAILABLE: u8 = 1;
/// `pickup_type` / `drop_off_type` values: phone the agency / ask the driver.
const ON_REQUEST: [u8; 2] = [2, 3];

pub(super) const RULES: &[Rule] = &[
    Rule {
        id: "unparseable_times",
        title: "Scheduled times are valid HH:MM:SS",
        category: Category::Schedule,
        run: unparseable_times,
    },
    Rule {
        id: "flex_calls",
        title: "On-demand (GTFS-Flex) calls",
        category: Category::Schedule,
        run: flex_calls,
    },
    Rule {
        id: "duplicate_sequences",
        title: "stop_sequence is unique within a trip",
        category: Category::Schedule,
        run: duplicate_sequences,
    },
    Rule {
        id: "time_travel",
        title: "Times never go backwards within a trip",
        category: Category::Schedule,
        run: time_travel,
    },
    Rule {
        id: "trips_without_calls",
        title: "Every trip has stop_times",
        category: Category::Schedule,
        run: trips_without_calls,
    },
    Rule {
        id: "short_trips",
        title: "Every trip serves at least two timed stops",
        category: Category::Schedule,
        run: short_trips,
    },
    Rule {
        id: "implausible_speed",
        title: "Speeds between consecutive stops are plausible for the mode",
        category: Category::Schedule,
        run: implausible_speed,
    },
    Rule {
        id: "boarding_restrictions",
        title: "Boarding and alighting restrictions (pickup_type / drop_off_type)",
        category: Category::Schedule,
        run: boarding_restrictions,
    },
];

/// Both times empty on an on-demand or approximate call: valid GTFS, the
/// vehicle is not scheduled there to the minute.
fn is_untimed(st: &StopTime) -> bool {
    st.arrival_time.trim().is_empty()
        && st.departure_time.trim().is_empty()
        && (st.has_pickup_window || st.timepoint == 0)
}

/// Arrival and departure in seconds, when both parse.
fn times(st: &StopTime) -> Option<(u32, u32)> {
    Some((
        gtfs::parse_time(&st.arrival_time)?,
        gtfs::parse_time(&st.departure_time)?,
    ))
}

fn unparseable_times(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for trip in &ctx.trips {
        for st in &trip.calls {
            if !is_untimed(st) && times(st).is_none() {
                c.push(|| {
                    format!(
                        "trip={} seq={} arr={:?} dep={:?}",
                        trip.trip_id, st.stop_sequence, st.arrival_time, st.departure_time
                    )
                });
            }
        }
    }
    c.finding(
        Severity::Error,
        "Stop times with unparseable arrival/departure time",
    )
    .into_iter()
    .collect()
}

/// Valid, but worth knowing: the RAPTOR router only uses timed calls, so
/// these are not offered as journeys.
fn flex_calls(ctx: &Ctx) -> Vec<Finding> {
    let mut calls = 0;
    let mut trips = Collector::default();
    for trip in &ctx.trips {
        let flex = trip.calls.iter().filter(|st| is_untimed(st)).count();
        if flex > 0 {
            calls += flex;
            trips.push(|| format!("{} ({flex} calls)", trip.trip_id));
        }
    }
    trips
        .finding(
            Severity::Info,
            format!("{calls} on-demand or untimed calls (GTFS-Flex): not used for routing"),
        )
        .into_iter()
        .collect()
}

fn duplicate_sequences(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for trip in &ctx.trips {
        // Calls are sorted by stop_sequence: duplicates are adjacent.
        if let Some(pair) = trip
            .calls
            .windows(2)
            .find(|w| w[0].stop_sequence == w[1].stop_sequence)
        {
            c.push(|| format!("trip={} seq={}", trip.trip_id, pair[0].stop_sequence));
        }
    }
    c.finding(Severity::Error, "Trips with duplicate stop_sequence values")
        .into_iter()
        .collect()
}

/// A departure before its arrival, or an arrival before the previous
/// departure. The router assumes time moves forward along a trip.
fn time_travel(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for trip in &ctx.trips {
        let mut previous: Option<(&StopTime, u32)> = None;
        for st in &trip.calls {
            let Some((arr, dep)) = times(st) else {
                continue;
            };
            if dep < arr {
                c.push(|| {
                    format!(
                        "trip={} seq={}: departs {} before arriving {}",
                        trip.trip_id, st.stop_sequence, st.departure_time, st.arrival_time
                    )
                });
            }
            if let Some((prev, prev_dep)) = previous
                && arr < prev_dep
            {
                c.push(|| {
                    format!(
                        "trip={} seq {}→{}: arrives {} before leaving {}",
                        trip.trip_id,
                        prev.stop_sequence,
                        st.stop_sequence,
                        st.arrival_time,
                        prev.departure_time
                    )
                });
            }
            previous = Some((st, dep));
        }
    }
    c.finding(
        Severity::Error,
        "Stop times going back in time within a trip",
    )
    .into_iter()
    .collect()
}

fn trips_without_calls(ctx: &Ctx) -> Vec<Finding> {
    let with_calls: FxHashSet<&str> = ctx.trips.iter().map(|t| t.trip_id).collect();
    let mut without: Vec<&str> = ctx
        .gtfs
        .trips
        .keys()
        .map(String::as_str)
        .filter(|id| !with_calls.contains(id))
        .collect();
    without.sort_unstable();
    let mut c = Collector::default();
    for trip_id in without {
        c.push(|| trip_id.to_string());
    }
    c.finding(Severity::Warning, "Trips without any stop_times")
        .into_iter()
        .collect()
}

/// Fewer than two timed calls: nothing can be ridden from one stop to another.
/// Fully on-demand trips are reported by `flex_calls` instead.
fn short_trips(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for trip in &ctx.trips {
        let timed = trip.calls.iter().filter(|st| times(st).is_some()).count();
        let all_untimed = trip.calls.iter().all(|st| is_untimed(st));
        if timed < 2 && !all_untimed {
            c.push(|| format!("{} ({timed} timed call(s))", trip.trip_id));
        }
    }
    c.finding(Severity::Warning, "Trips with fewer than two timed stops")
        .into_iter()
        .collect()
}

/// Upper bound for a commercial speed, in km/h, per GTFS `route_type`
/// (base values and the extended types of the Google Transit spec).
fn max_speed_kmh(route_type: u16) -> f64 {
    match route_type {
        0 | 900..=906 => 100.0,                    // tram, light rail
        1 | 12 | 400..=405 => 120.0,               // metro, monorail, urban rail
        2 | 100..=117 => 350.0,                    // rail, high-speed rail
        3 | 11 | 700..=716 | 800 => 120.0,         // bus, trolleybus
        200..=209 => 150.0,                        // coach
        4 | 1000 | 1200 => 80.0,                   // ferry, water transport
        5..=7 | 1300..=1307 | 1400..=1402 => 60.0, // cable, gondola, funicular
        _ => 350.0,                                // unknown: only flag the absurd
    }
}

/// Coordinates of a stop, when it exists and is located.
fn position(ctx: &Ctx, stop_id: &str) -> Option<(f64, f64)> {
    let stop = ctx.gtfs.stops.get(stop_id)?;
    (stop.stop_lat != 0.0 && stop.stop_lon != 0.0).then_some((stop.stop_lat, stop.stop_lon))
}

fn route_type_of(ctx: &Ctx, trip: &TripCalls) -> Option<u16> {
    let route_id = &ctx.gtfs.trips.get(trip.trip_id)?.route_id;
    Some(ctx.gtfs.routes.get(route_id)?.route_type)
}

/// A vehicle faster than its mode allows between two consecutive stops:
/// usually a misplaced stop or a wrong time rather than a fast vehicle.
fn implausible_speed(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for trip in &ctx.trips {
        let Some(route_type) = route_type_of(ctx, trip) else {
            continue; // Unknown trip or route: reported by the integrity rules.
        };
        let max_mps = max_speed_kmh(route_type) / 3.6;
        let timed: Vec<(&StopTime, u32, u32)> = trip
            .calls
            .iter()
            .filter_map(|st| times(st).map(|(arr, dep)| (*st, arr, dep)))
            .collect();
        for pair in timed.windows(2) {
            let ((from, _, dep), (to, arr, _)) = (pair[0], pair[1]);
            let (Some((lat1, lon1)), Some((lat2, lon2))) =
                (position(ctx, &from.stop_id), position(ctx, &to.stop_id))
            else {
                continue;
            };
            let Some(elapsed) = arr.checked_sub(dep) else {
                continue; // Going back in time: reported by `time_travel`.
            };
            let meters = haversine_meters(lat1, lon1, lat2, lon2);
            let speed = meters / (f64::from(elapsed) + MINUTE_ROUNDING_S);
            if speed > max_mps {
                c.push(|| {
                    format!(
                        "trip={} seq {}→{}: {:.1} km in {} min (route_type {route_type}, ≥ {:.0} km/h)",
                        trip.trip_id,
                        from.stop_sequence,
                        to.stop_sequence,
                        meters / 1000.0,
                        elapsed / 60,
                        speed * 3.6
                    )
                });
            }
        }
    }
    c.finding(
        Severity::Warning,
        "Implausible speed between consecutive stops for the route's mode",
    )
    .into_iter()
    .collect()
}

/// Calls where boarding or alighting is forbidden or on request. Forbidding
/// alighting at the first stop and boarding at the last is normal and not
/// counted. The router does not read these fields yet.
fn boarding_restrictions(ctx: &Ctx) -> Vec<Finding> {
    let mut forbidden = Collector::default();
    let mut on_request = Collector::default();
    for trip in &ctx.trips {
        let last = trip.calls.len().saturating_sub(1);
        for (idx, st) in trip.calls.iter().enumerate() {
            let no_boarding = st.pickup_type == NOT_AVAILABLE && idx != last;
            let no_alighting = st.drop_off_type == NOT_AVAILABLE && idx != 0;
            if no_boarding || no_alighting {
                let what = match (no_boarding, no_alighting) {
                    (true, true) => "no boarding nor alighting",
                    (true, false) => "no boarding",
                    _ => "no alighting",
                };
                forbidden.push(|| {
                    format!(
                        "trip={} seq={} stop={}: {what}",
                        trip.trip_id, st.stop_sequence, st.stop_id
                    )
                });
            }
            if ON_REQUEST.contains(&st.pickup_type) || ON_REQUEST.contains(&st.drop_off_type) {
                on_request.push(|| {
                    format!(
                        "trip={} seq={} stop={}",
                        trip.trip_id, st.stop_sequence, st.stop_id
                    )
                });
            }
        }
    }
    [
        forbidden.finding(
            Severity::Info,
            "Intermediate calls forbidding boarding or alighting (pickup_type/drop_off_type = 1)",
        ),
        on_request.finding(
            Severity::Info,
            "Calls served on request (pickup_type/drop_off_type = 2 or 3)",
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}
