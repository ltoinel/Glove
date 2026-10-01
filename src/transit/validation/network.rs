//! The network around the schedule: stops, routes, transfers and pathways.

use rustc_hash::{FxHashMap, FxHashSet};

use super::{Category, Collector, Ctx, Finding, Rule, Severity};
use crate::shared::util::haversine_meters;
use crate::transit::gtfs::Route;

/// `min_transfer_time` beyond this is suspect (seconds).
const MAX_TRANSFER_TIME_S: u32 = 1800;
/// Walking faster than this to make a transfer is implausible (m/s; a brisk
/// walk is about 1.4 m/s).
const MAX_WALK_SPEED_MPS: f64 = 2.5;
/// A transfer longer than this is a trip of its own (meters).
const MAX_TRANSFER_DISTANCE_M: f64 = 1500.0;

pub(super) const RULES: &[Rule] = &[
    Rule {
        id: "stop_coordinates",
        title: "Stops have valid coordinates",
        category: Category::Coordinates,
        run: stop_coordinates,
    },
    Rule {
        id: "stops_outside_area",
        title: "Stops lie inside the configured map area",
        category: Category::Coordinates,
        run: stops_outside_area,
    },
    Rule {
        id: "stop_names",
        title: "Stops have a name",
        category: Category::Display,
        run: stop_names,
    },
    Rule {
        id: "stops_unused",
        title: "Every platform is served by a trip",
        category: Category::ReferentialIntegrity,
        run: stops_unused,
    },
    Rule {
        id: "ungrouped_stops",
        title: "Same-name stops are grouped under a station",
        category: Category::Transfers,
        run: ungrouped_stops,
    },
    Rule {
        id: "routes_without_trips",
        title: "Every route has trips",
        category: Category::ReferentialIntegrity,
        run: routes_without_trips,
    },
    Rule {
        id: "route_type_unknown",
        title: "route_type is a known GTFS mode",
        category: Category::ReferentialIntegrity,
        run: route_type_unknown,
    },
    Rule {
        id: "route_colors",
        title: "Route colors are valid hex",
        category: Category::Display,
        run: route_colors,
    },
    Rule {
        id: "empty_headsigns",
        title: "Trips have a headsign",
        category: Category::Display,
        run: empty_headsigns,
    },
    Rule {
        id: "transfer_stop_ref",
        title: "Transfers reference existing stops",
        category: Category::Transfers,
        run: transfer_stop_ref,
    },
    Rule {
        id: "transfer_times",
        title: "min_transfer_time is plausible",
        category: Category::Transfers,
        run: transfer_times,
    },
    Rule {
        id: "transfer_walk_speed",
        title: "Transfers can be walked in their min_transfer_time",
        category: Category::Transfers,
        run: transfer_walk_speed,
    },
    Rule {
        id: "isolated_siblings",
        title: "Stops of a station are connected to each other",
        category: Category::Transfers,
        run: isolated_siblings,
    },
    Rule {
        id: "pathway_stop_ref",
        title: "Pathways reference existing stops",
        category: Category::Pathways,
        run: pathway_stop_ref,
    },
    Rule {
        id: "pathway_times",
        title: "Pathways have a traversal_time",
        category: Category::Pathways,
        run: pathway_times,
    },
    Rule {
        id: "pathway_mode",
        title: "pathway_mode is a known value (1-7)",
        category: Category::Pathways,
        run: pathway_mode,
    },
];

fn stop_coordinates(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for stop in &ctx.stops {
        let (lat, lon) = (stop.stop_lat, stop.stop_lon);
        if lat == 0.0
            || lon == 0.0
            || !(-90.0..=90.0).contains(&lat)
            || !(-180.0..=180.0).contains(&lon)
        {
            c.push(|| format!("{} ({})", stop.stop_id, stop.stop_name));
        }
    }
    c.finding(
        Severity::Warning,
        "Stops with zero or out-of-range coordinates",
    )
    .into_iter()
    .collect()
}

/// Catches swapped lat/lon and stops placed far away, which the range check
/// above lets through.
fn stops_outside_area(ctx: &Ctx) -> Vec<Finding> {
    let Some(bounds) = ctx.input.bounds else {
        return vec![];
    };
    let mut c = Collector::default();
    for stop in &ctx.stops {
        let (lat, lon) = (stop.stop_lat, stop.stop_lon);
        let located = lat != 0.0 && lon != 0.0;
        if located && !bounds.contains(lat, lon) {
            c.push(|| {
                format!(
                    "{} ({}) at {lat:.5}, {lon:.5}",
                    stop.stop_id, stop.stop_name
                )
            });
        }
    }
    c.finding(
        Severity::Warning,
        "Stops outside the configured map area (map.bounds_*)",
    )
    .into_iter()
    .collect()
}

fn stop_names(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for stop in &ctx.stops {
        if stop.stop_name.trim().is_empty() {
            c.push(|| stop.stop_id.clone());
        }
    }
    c.finding(Severity::Warning, "Stops with empty stop_name")
        .into_iter()
        .collect()
}

/// Platforms (`location_type` 0) no trip calls at. Needs `location_type`:
/// without it, stations and entrances would all look unused.
fn stops_unused(ctx: &Ctx) -> Vec<Finding> {
    if ctx.input.location_types.is_empty() {
        return vec![];
    }
    let served: FxHashSet<&str> = ctx
        .gtfs
        .stop_times
        .iter()
        .map(|st| st.stop_id.as_str())
        .collect();
    let mut c = Collector::default();
    for stop in &ctx.stops {
        let platform = ctx.location_type(&stop.stop_id) == Some(0);
        if platform && !served.contains(stop.stop_id.as_str()) {
            c.push(|| format!("{} ({})", stop.stop_id, stop.stop_name));
        }
    }
    c.finding(Severity::Info, "Platforms served by no trip")
        .into_iter()
        .collect()
}

fn ungrouped_stops(ctx: &Ctx) -> Vec<Finding> {
    let mut by_name: FxHashMap<&str, usize> = FxHashMap::default();
    for stop in &ctx.stops {
        if stop.parent_station.is_empty() && !stop.stop_name.is_empty() {
            *by_name.entry(&stop.stop_name).or_default() += 1;
        }
    }
    let mut names: Vec<(&str, usize)> = by_name.into_iter().filter(|&(_, n)| n > 2).collect();
    names.sort_unstable();
    let mut c = Collector::default();
    for (name, n) in names {
        c.push(|| format!("{name} ({n} stops)"));
    }
    c.finding(
        Severity::Info,
        "Multiple stops share the same name without parent_station grouping",
    )
    .into_iter()
    .collect()
}

fn sorted_routes<'a>(ctx: &Ctx<'a>) -> Vec<&'a Route> {
    let mut routes: Vec<&Route> = ctx.gtfs.routes.values().collect();
    routes.sort_unstable_by(|a, b| a.route_id.cmp(&b.route_id));
    routes
}

fn routes_without_trips(ctx: &Ctx) -> Vec<Finding> {
    let used: FxHashSet<&str> = ctx
        .gtfs
        .trips
        .values()
        .map(|t| t.route_id.as_str())
        .collect();
    let mut c = Collector::default();
    for route in sorted_routes(ctx) {
        if !used.contains(route.route_id.as_str()) {
            c.push(|| format!("{} ({})", route.route_id, route.route_short_name));
        }
    }
    c.finding(Severity::Warning, "Routes without any trip")
        .into_iter()
        .collect()
}

/// Base GTFS values and the extended route types (Google Transit), used for
/// mode-dependent behavior such as `routing.prefer_rail`.
fn is_known_route_type(route_type: u16) -> bool {
    matches!(
        route_type,
        0..=7
            | 11
            | 12
            | 100..=117
            | 200..=209
            | 400..=405
            | 700..=716
            | 800
            | 900..=906
            | 1000
            | 1100
            | 1200
            | 1300..=1307
            | 1400..=1402
            | 1500..=1507
            | 1700..=1702
    )
}

fn route_type_unknown(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for route in sorted_routes(ctx) {
        if !is_known_route_type(route.route_type) {
            c.push(|| format!("{} route_type={}", route.route_id, route.route_type));
        }
    }
    c.finding(Severity::Warning, "Routes with an unknown route_type")
        .into_iter()
        .collect()
}

fn route_colors(ctx: &Ctx) -> Vec<Finding> {
    let is_hex = |color: &str| color.len() == 6 && color.chars().all(|ch| ch.is_ascii_hexdigit());
    let mut c = Collector::default();
    for route in sorted_routes(ctx) {
        if !route.route_color.is_empty() && !is_hex(&route.route_color) {
            c.push(|| format!("{} color={}", route.route_id, route.route_color));
        }
    }
    c.finding(Severity::Warning, "Routes with invalid hex color")
        .into_iter()
        .collect()
}

fn empty_headsigns(ctx: &Ctx) -> Vec<Finding> {
    let count = ctx
        .gtfs
        .trips
        .values()
        .filter(|t| t.trip_headsign.trim().is_empty())
        .count();
    if count == 0 {
        return vec![];
    }
    vec![Finding {
        severity: Severity::Info,
        message: "Trips with empty trip_headsign".into(),
        count,
        samples: vec![],
    }]
}

fn transfer_stop_ref(ctx: &Ctx) -> Vec<Finding> {
    let known = |id: &String| ctx.gtfs.stops.contains_key(id);
    let mut c = Collector::default();
    for t in &ctx.gtfs.transfers {
        if !known(&t.from_stop_id) || !known(&t.to_stop_id) {
            c.push(|| format!("{} → {}", t.from_stop_id, t.to_stop_id));
        }
    }
    c.finding(Severity::Error, "Transfers reference non-existent stops")
        .into_iter()
        .collect()
}

fn transfer_times(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for t in &ctx.gtfs.transfers {
        if let Some(time) = t.min_transfer_time
            && (time == 0 || time > MAX_TRANSFER_TIME_S)
        {
            c.push(|| format!("{} → {} ({time}s)", t.from_stop_id, t.to_stop_id));
        }
    }
    c.finding(
        Severity::Warning,
        "Transfers with suspect min_transfer_time (0s or >30min)",
    )
    .into_iter()
    .collect()
}

/// The straight-line distance between the two stops, walked in
/// `min_transfer_time`, must be humanly possible — and the walk short enough
/// to be a transfer at all.
fn transfer_walk_speed(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for t in &ctx.gtfs.transfers {
        let (Some(from), Some(to)) = (
            ctx.gtfs.stops.get(&t.from_stop_id),
            ctx.gtfs.stops.get(&t.to_stop_id),
        ) else {
            continue; // Reported by `transfer_stop_ref`.
        };
        let located = |lat: f64, lon: f64| lat != 0.0 && lon != 0.0;
        if t.from_stop_id == t.to_stop_id
            || !located(from.stop_lat, from.stop_lon)
            || !located(to.stop_lat, to.stop_lon)
        {
            continue;
        }
        let meters = haversine_meters(from.stop_lat, from.stop_lon, to.stop_lat, to.stop_lon);
        let too_fast = t
            .min_transfer_time
            .filter(|&time| time > 0)
            .is_some_and(|time| meters / f64::from(time) > MAX_WALK_SPEED_MPS);
        if too_fast || meters > MAX_TRANSFER_DISTANCE_M {
            let time = t
                .min_transfer_time
                .map_or_else(|| "no time".to_string(), |s| format!("{s}s"));
            c.push(|| {
                format!(
                    "{} → {}: {meters:.0} m in {time}",
                    t.from_stop_id, t.to_stop_id
                )
            });
        }
    }
    c.finding(
        Severity::Warning,
        "Transfers longer than 1.5 km or faster than 2.5 m/s on foot",
    )
    .into_iter()
    .collect()
}

fn isolated_siblings(ctx: &Ctx) -> Vec<Finding> {
    let mut by_parent: FxHashMap<&str, Vec<&str>> = FxHashMap::default();
    for stop in &ctx.stops {
        if !stop.parent_station.is_empty() {
            by_parent
                .entry(&stop.parent_station)
                .or_default()
                .push(&stop.stop_id);
        }
    }
    let connected: FxHashSet<&str> = ctx
        .gtfs
        .transfers
        .iter()
        .flat_map(|t| [t.from_stop_id.as_str(), t.to_stop_id.as_str()])
        .chain(
            ctx.gtfs
                .pathways
                .iter()
                .flat_map(|p| [p.from_stop_id.as_str(), p.to_stop_id.as_str()]),
        )
        .collect();
    let mut isolated: Vec<(&str, &str)> = by_parent
        .iter()
        .filter(|(_, children)| children.len() > 1)
        .flat_map(|(&parent, children)| {
            children
                .iter()
                .filter(|&&child| !connected.contains(child))
                .map(move |&child| (child, parent))
        })
        .collect();
    isolated.sort_unstable();
    let mut c = Collector::default();
    for (child, parent) in isolated {
        c.push(|| format!("{child} (parent={parent})"));
    }
    c.finding(
        Severity::Warning,
        "Stops in multi-stop stations with no transfer or pathway to siblings",
    )
    .into_iter()
    .collect()
}

fn pathway_stop_ref(ctx: &Ctx) -> Vec<Finding> {
    let known = |id: &String| ctx.gtfs.stops.contains_key(id);
    let mut c = Collector::default();
    for p in &ctx.gtfs.pathways {
        if !known(&p.from_stop_id) || !known(&p.to_stop_id) {
            c.push(|| format!("{} → {}", p.from_stop_id, p.to_stop_id));
        }
    }
    c.finding(Severity::Error, "Pathways reference non-existent stops")
        .into_iter()
        .collect()
}

fn pathway_times(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for p in &ctx.gtfs.pathways {
        if p.traversal_time.is_none_or(|time| time == 0) {
            c.push(|| format!("{} → {}", p.from_stop_id, p.to_stop_id));
        }
    }
    c.finding(
        Severity::Warning,
        "Pathways with missing or zero traversal_time",
    )
    .into_iter()
    .collect()
}

/// 1 walkway, 2 stairs, 3 moving sidewalk, 4 escalator, 5 elevator,
/// 6 fare gate, 7 exit gate. Anything else cannot be costed.
fn pathway_mode(ctx: &Ctx) -> Vec<Finding> {
    let mut c = Collector::default();
    for p in &ctx.gtfs.pathways {
        if !(1..=7).contains(&p.pathway_mode) {
            c.push(|| {
                format!(
                    "{} → {} (pathway_mode={})",
                    p.from_stop_id, p.to_stop_id, p.pathway_mode
                )
            });
        }
    }
    c.finding(Severity::Error, "Pathways with an unknown pathway_mode")
        .into_iter()
        .collect()
}
