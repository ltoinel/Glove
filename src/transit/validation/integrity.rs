//! References between files, IDs the loader overwrote or dropped, and
//! consistency of stop types (`location_type`).

use rustc_hash::FxHashSet;

use super::{Category, Collector, Ctx, Finding, Rule, Severity};

pub(super) const RULES: &[Rule] = &[
    Rule {
        id: "malformed_rows",
        title: "Every row can be parsed",
        category: Category::Loading,
        run: malformed_rows,
    },
    Rule {
        id: "duplicate_ids",
        title: "IDs are unique within their file",
        category: Category::Loading,
        run: duplicate_ids,
    },
    Rule {
        id: "stop_times_trip_ref",
        title: "stop_times reference existing trips",
        category: Category::ReferentialIntegrity,
        run: stop_times_trip_ref,
    },
    Rule {
        id: "stop_times_stop_ref",
        title: "stop_times reference existing stops",
        category: Category::ReferentialIntegrity,
        run: stop_times_stop_ref,
    },
    Rule {
        id: "trips_route_ref",
        title: "trips reference existing routes",
        category: Category::ReferentialIntegrity,
        run: trips_route_ref,
    },
    Rule {
        id: "trips_service_ref",
        title: "trips reference defined services",
        category: Category::ReferentialIntegrity,
        run: trips_service_ref,
    },
    Rule {
        id: "routes_agency_ref",
        title: "routes reference existing agencies",
        category: Category::ReferentialIntegrity,
        run: routes_agency_ref,
    },
    Rule {
        id: "parent_station_ref",
        title: "parent_station references existing stops",
        category: Category::ReferentialIntegrity,
        run: parent_station_ref,
    },
    Rule {
        id: "stop_times_location_type",
        title: "stop_times serve platforms, not stations or entrances",
        category: Category::ReferentialIntegrity,
        run: stop_times_location_type,
    },
    Rule {
        id: "parent_station_type",
        title: "Station hierarchy follows location_type",
        category: Category::ReferentialIntegrity,
        run: parent_station_type,
    },
];

fn malformed_rows(ctx: &Ctx) -> Vec<Finding> {
    let per_file = &ctx.gtfs.load_report.malformed_rows;
    let count: u64 = per_file.iter().map(|(_, rows)| rows).sum();
    if count == 0 {
        return vec![];
    }
    // One sample per file: the rows themselves are gone.
    vec![Finding {
        severity: Severity::Warning,
        message: "Rows skipped at load because they could not be parsed".into(),
        count: usize::try_from(count).unwrap_or(usize::MAX),
        samples: per_file
            .iter()
            .map(|(file, rows)| format!("{file}: {rows} rows"))
            .collect(),
    }]
}

fn duplicate_ids(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for (file, ids) in &ctx.gtfs.load_report.duplicate_ids {
        for id in ids {
            c.push(|| format!("{file}: {id}"));
        }
    }
    c.finding(
        Severity::Error,
        "Duplicate IDs: only the last row of each is kept",
    )
    .into_iter()
    .collect()
}

fn stop_times_trip_ref(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for trip in &ctx.trips {
        if !ctx.gtfs.trips.contains_key(trip.trip_id) {
            c.push(|| trip.trip_id.to_string());
        }
    }
    c.finding(Severity::Error, "stop_times reference non-existent trip_id")
        .into_iter()
        .collect()
}

fn stop_times_stop_ref(ctx: &Ctx) -> Vec<Finding> {
    let missing: FxHashSet<&str> = ctx
        .gtfs
        .stop_times
        .iter()
        .filter(|st| !ctx.gtfs.stops.contains_key(&st.stop_id))
        .map(|st| st.stop_id.as_str())
        .collect();
    let mut missing: Vec<&str> = missing.into_iter().collect();
    missing.sort_unstable();
    let mut c = Collector::default();
    for stop_id in missing {
        c.push(|| stop_id.to_string());
    }
    c.finding(Severity::Error, "stop_times reference non-existent stop_id")
        .into_iter()
        .collect()
}

fn trips_route_ref(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for trip in sorted_trips(ctx) {
        if !ctx.gtfs.routes.contains_key(&trip.route_id) {
            c.push(|| format!("trip={} route={}", trip.trip_id, trip.route_id));
        }
    }
    c.finding(Severity::Error, "trips reference non-existent route_id")
        .into_iter()
        .collect()
}

fn trips_service_ref(ctx: &Ctx) -> Vec<Finding> {
    let exception_services: FxHashSet<&str> = ctx
        .gtfs
        .calendar_dates
        .iter()
        .map(|cd| cd.service_id.as_str())
        .collect();
    let mut c = Collector::default();
    for trip in sorted_trips(ctx) {
        if !ctx.gtfs.calendars.contains_key(&trip.service_id)
            && !exception_services.contains(trip.service_id.as_str())
        {
            c.push(|| format!("trip={} service={}", trip.trip_id, trip.service_id));
        }
    }
    c.finding(
        Severity::Error,
        "trips reference service_id not in calendar or calendar_dates",
    )
    .into_iter()
    .collect()
}

/// `agency_id` may be empty only when the feed has a single agency.
fn routes_agency_ref(ctx: &Ctx) -> Vec<Finding> {
    let agencies: FxHashSet<&str> = ctx
        .gtfs
        .agencies
        .iter()
        .map(|a| a.agency_id.as_str())
        .collect();
    let mut unknown = Collector::default();
    let mut missing = Collector::default();
    let mut routes: Vec<_> = ctx.gtfs.routes.values().collect();
    routes.sort_unstable_by(|a, b| a.route_id.cmp(&b.route_id));
    for route in routes {
        if route.agency_id.is_empty() {
            if agencies.len() > 1 {
                missing.push(|| route.route_id.clone());
            }
        } else if !agencies.contains(route.agency_id.as_str()) {
            unknown.push(|| format!("route={} agency={}", route.route_id, route.agency_id));
        }
    }
    [
        unknown.finding(Severity::Error, "routes reference non-existent agency_id"),
        missing.finding(
            Severity::Warning,
            "routes without agency_id in a feed with several agencies",
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn parent_station_ref(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for stop in &ctx.stops {
        if !stop.parent_station.is_empty() && !ctx.gtfs.stops.contains_key(&stop.parent_station) {
            c.push(|| format!("{} → parent={}", stop.stop_id, stop.parent_station));
        }
    }
    c.finding(
        Severity::Error,
        "Stops reference non-existent parent_station",
    )
    .into_iter()
    .collect()
}

/// Vehicles stop at platforms (`location_type` 0). A call at a station,
/// entrance or node cannot be boarded.
fn stop_times_location_type(ctx: &Ctx) -> Vec<Finding> {
    if ctx.input.location_types.is_empty() {
        return vec![];
    }
    let wrong: FxHashSet<&str> = ctx
        .gtfs
        .stop_times
        .iter()
        .filter(|st| ctx.location_type(&st.stop_id).is_some_and(|lt| lt != 0))
        .map(|st| st.stop_id.as_str())
        .collect();
    let mut wrong: Vec<&str> = wrong.into_iter().collect();
    wrong.sort_unstable();
    let mut c = Collector::default();
    for stop_id in wrong {
        let lt = ctx.location_type(stop_id).unwrap_or_default();
        c.push(|| format!("{stop_id} (location_type={lt})"));
    }
    c.finding(
        Severity::Error,
        "stop_times reference stops that are not platforms (location_type ≠ 0)",
    )
    .into_iter()
    .collect()
}

/// Platforms, entrances and nodes belong to a station (1); boarding areas
/// belong to a platform (0); a station has no parent.
fn parent_station_type(ctx: &Ctx) -> Vec<Finding> {
    if ctx.input.location_types.is_empty() {
        return vec![];
    }
    let mut c = Collector::default();
    for stop in &ctx.stops {
        let Some(own) = ctx.location_type(&stop.stop_id) else {
            continue;
        };
        if stop.parent_station.is_empty() {
            continue;
        }
        let Some(parent) = ctx.location_type(&stop.parent_station) else {
            continue; // Missing parent: reported by `parent_station_ref`.
        };
        let expected = match own {
            1 => None,
            4 => Some(0),
            _ => Some(1),
        };
        if expected != Some(parent) {
            c.push(|| {
                format!(
                    "{} (location_type={own}) → parent {} (location_type={parent})",
                    stop.stop_id, stop.parent_station
                )
            });
        }
    }
    c.finding(
        Severity::Warning,
        "parent_station points to a stop of the wrong location_type",
    )
    .into_iter()
    .collect()
}

fn sorted_trips<'a>(ctx: &Ctx<'a>) -> Vec<&'a crate::transit::gtfs::Trip> {
    let mut trips: Vec<_> = ctx.gtfs.trips.values().collect();
    trips.sort_unstable_by(|a, b| a.trip_id.cmp(&b.trip_id));
    trips
}
