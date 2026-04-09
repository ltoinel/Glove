//! GTFS data validation and management endpoints.

use actix_web::{HttpResponse, get, post, web};
use arc_swap::ArcSwap;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

use crate::config::AppConfig;
use crate::gtfs::GtfsData;
use crate::raptor::RaptorData;

// ---------------------------------------------------------------------------
// Validation types
// ---------------------------------------------------------------------------

/// Severity level for a validation issue.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Category of a validation issue.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    ReferentialIntegrity,
    Calendar,
    Coordinates,
    Transfers,
    Pathways,
    Display,
}

/// A single validation issue found in the GTFS data.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    /// Number of affected entities.
    pub count: usize,
    /// Sample IDs illustrating the issue (max 5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<String>,
}

/// Summary counts for each severity level.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct ValidationSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub total_checks: usize,
}

/// Response for `GET /api/gtfs/validate`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ValidateResponse {
    pub summary: ValidationSummary,
    pub issues: Vec<ValidationIssue>,
}

/// Response for `POST /api/gtfs/reload`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReloadResponse {
    pub status: String,
    pub loaded_at: String,
    pub gtfs: super::status::GtfsStats,
    pub raptor: super::status::RaptorStats,
}

// ---------------------------------------------------------------------------
// Validation logic
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Individual validation checks
// ---------------------------------------------------------------------------

/// Helper: build an issue from collected samples, or return empty if none found.
fn issue_from_samples(
    samples: Vec<String>,
    severity: Severity,
    category: Category,
    message: &str,
) -> Vec<ValidationIssue> {
    if samples.is_empty() {
        return vec![];
    }
    let count = samples.len();
    vec![ValidationIssue {
        severity,
        category,
        message: message.into(),
        count,
        samples: samples.into_iter().take(5).collect(),
    }]
}

fn check_stop_times_trips(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let orphans: Vec<String> = gtfs
        .stop_times
        .iter()
        .filter(|st| !gtfs.trips.contains_key(&st.trip_id))
        .map(|st| st.trip_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    issue_from_samples(
        orphans,
        Severity::Error,
        Category::ReferentialIntegrity,
        "stop_times reference non-existent trip_id",
    )
}

fn check_stop_times_stops(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let orphans: Vec<String> = gtfs
        .stop_times
        .iter()
        .filter(|st| !gtfs.stops.contains_key(&st.stop_id))
        .map(|st| st.stop_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    issue_from_samples(
        orphans,
        Severity::Error,
        Category::ReferentialIntegrity,
        "stop_times reference non-existent stop_id",
    )
}

fn check_trips_routes(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let orphans: Vec<String> = gtfs
        .trips
        .values()
        .filter(|t| !gtfs.routes.contains_key(&t.route_id))
        .map(|t| format!("trip={} route={}", t.trip_id, t.route_id))
        .collect();
    issue_from_samples(
        orphans,
        Severity::Error,
        Category::ReferentialIntegrity,
        "trips reference non-existent route_id",
    )
}

fn check_trips_calendars(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let cal_date_services: std::collections::HashSet<&str> = gtfs
        .calendar_dates
        .iter()
        .map(|cd| cd.service_id.as_str())
        .collect();
    let orphans: Vec<String> = gtfs
        .trips
        .values()
        .filter(|t| {
            !gtfs.calendars.contains_key(&t.service_id)
                && !cal_date_services.contains(t.service_id.as_str())
        })
        .map(|t| format!("trip={} service={}", t.trip_id, t.service_id))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    issue_from_samples(
        orphans,
        Severity::Error,
        Category::ReferentialIntegrity,
        "trips reference service_id not in calendar or calendar_dates",
    )
}

fn check_calendar_coverage(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let today = chrono::Utc::now().format("%Y%m%d").to_string();
    let active = gtfs
        .calendars
        .values()
        .filter(|c| c.start_date <= today && c.end_date >= today)
        .count();
    if active == 0 && !gtfs.calendars.is_empty() {
        let samples = gtfs
            .calendars
            .values()
            .take(3)
            .map(|c| format!("{}: {} → {}", c.service_id, c.start_date, c.end_date))
            .collect();
        return vec![ValidationIssue {
            severity: Severity::Error,
            category: Category::Calendar,
            message: "No calendar covers today's date — no trips will be active".into(),
            count: gtfs.calendars.len(),
            samples,
        }];
    }
    if active > 0 {
        let total = gtfs.calendars.len();
        let inactive = total - active;
        if inactive > 0 {
            return vec![ValidationIssue {
                severity: Severity::Info,
                category: Category::Calendar,
                message: format!("{active}/{total} calendars active today"),
                count: inactive,
                samples: vec![],
            }];
        }
    }
    vec![]
}

fn check_calendar_dates(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let inverted: Vec<String> = gtfs
        .calendars
        .values()
        .filter(|c| c.start_date > c.end_date)
        .map(|c| format!("{}: {} > {}", c.service_id, c.start_date, c.end_date))
        .collect();
    issue_from_samples(
        inverted,
        Severity::Error,
        Category::Calendar,
        "Calendar start_date is after end_date",
    )
}

fn check_coordinates(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let bad: Vec<String> = gtfs
        .stops
        .values()
        .filter(|s| {
            s.stop_lat == 0.0
                || s.stop_lon == 0.0
                || s.stop_lat < -90.0
                || s.stop_lat > 90.0
                || s.stop_lon < -180.0
                || s.stop_lon > 180.0
        })
        .map(|s| format!("{} ({})", s.stop_id, s.stop_name))
        .collect();
    issue_from_samples(
        bad,
        Severity::Warning,
        Category::Coordinates,
        "Stops with zero or out-of-range coordinates",
    )
}

fn check_stop_names(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let unnamed: Vec<String> = gtfs
        .stops
        .values()
        .filter(|s| s.stop_name.trim().is_empty())
        .map(|s| s.stop_id.clone())
        .collect();
    issue_from_samples(
        unnamed,
        Severity::Warning,
        Category::Display,
        "Stops with empty stop_name",
    )
}

fn check_parent_stations(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let bad: Vec<String> = gtfs
        .stops
        .values()
        .filter(|s| !s.parent_station.is_empty() && !gtfs.stops.contains_key(&s.parent_station))
        .map(|s| format!("{} → parent={}", s.stop_id, s.parent_station))
        .collect();
    issue_from_samples(
        bad,
        Severity::Error,
        Category::ReferentialIntegrity,
        "Stops reference non-existent parent_station",
    )
}

fn check_transfer_stops(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let bad: Vec<String> = gtfs
        .transfers
        .iter()
        .filter(|t| {
            !gtfs.stops.contains_key(&t.from_stop_id) || !gtfs.stops.contains_key(&t.to_stop_id)
        })
        .map(|t| format!("{} → {}", t.from_stop_id, t.to_stop_id))
        .collect();
    issue_from_samples(
        bad,
        Severity::Error,
        Category::Transfers,
        "Transfers reference non-existent stops",
    )
}

fn check_transfer_times(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let suspect: Vec<String> = gtfs
        .transfers
        .iter()
        .filter(|t| {
            t.min_transfer_time
                .is_some_and(|time| time == 0 || time > 1800)
        })
        .map(|t| {
            format!(
                "{} → {} ({}s)",
                t.from_stop_id,
                t.to_stop_id,
                t.min_transfer_time.unwrap_or(0)
            )
        })
        .collect();
    issue_from_samples(
        suspect,
        Severity::Warning,
        Category::Transfers,
        "Transfers with suspect min_transfer_time (0s or >30min)",
    )
}

fn check_pathway_stops(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let bad: Vec<String> = gtfs
        .pathways
        .iter()
        .filter(|p| {
            !gtfs.stops.contains_key(&p.from_stop_id) || !gtfs.stops.contains_key(&p.to_stop_id)
        })
        .map(|p| format!("{} → {}", p.from_stop_id, p.to_stop_id))
        .collect();
    issue_from_samples(
        bad,
        Severity::Error,
        Category::Pathways,
        "Pathways reference non-existent stops",
    )
}

fn check_pathway_times(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let bad: Vec<String> = gtfs
        .pathways
        .iter()
        .filter(|p| p.traversal_time.is_none() || p.traversal_time == Some(0))
        .map(|p| format!("{} → {}", p.from_stop_id, p.to_stop_id))
        .collect();
    issue_from_samples(
        bad,
        Severity::Warning,
        Category::Pathways,
        "Pathways with missing or zero traversal_time",
    )
}

fn check_isolated_siblings(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    use std::collections::{HashMap, HashSet};
    let mut by_parent: HashMap<&str, Vec<&str>> = HashMap::new();
    for s in gtfs.stops.values() {
        if !s.parent_station.is_empty() {
            by_parent
                .entry(&s.parent_station)
                .or_default()
                .push(&s.stop_id);
        }
    }
    let mut connected: HashSet<&str> = HashSet::new();
    for t in &gtfs.transfers {
        connected.insert(&t.from_stop_id);
        connected.insert(&t.to_stop_id);
    }
    for p in &gtfs.pathways {
        connected.insert(&p.from_stop_id);
        connected.insert(&p.to_stop_id);
    }
    let isolated: Vec<String> = by_parent
        .iter()
        .filter(|(_, children)| children.len() > 1)
        .flat_map(|(parent, children)| {
            children
                .iter()
                .filter(|&&child| !connected.contains(child))
                .map(move |&child| format!("{child} (parent={parent})"))
        })
        .collect();
    issue_from_samples(
        isolated,
        Severity::Warning,
        Category::Transfers,
        "Stops in multi-stop stations with no transfer or pathway to siblings",
    )
}

fn check_ungrouped_stops(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    use std::collections::HashMap;
    let mut name_counts: HashMap<&str, Vec<&str>> = HashMap::new();
    for s in gtfs.stops.values() {
        if s.parent_station.is_empty() && !s.stop_name.is_empty() {
            name_counts
                .entry(&s.stop_name)
                .or_default()
                .push(&s.stop_id);
        }
    }
    let ungrouped: Vec<String> = name_counts
        .into_iter()
        .filter(|(_, ids)| ids.len() > 2)
        .map(|(name, ids)| format!("{name} ({} stops)", ids.len()))
        .collect();
    issue_from_samples(
        ungrouped,
        Severity::Info,
        Category::Transfers,
        "Multiple stops share the same name without parent_station grouping",
    )
}

fn check_duplicate_sequences(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    use std::collections::HashMap;
    let mut seq_by_trip: HashMap<&str, Vec<u32>> = HashMap::new();
    for st in &gtfs.stop_times {
        seq_by_trip
            .entry(&st.trip_id)
            .or_default()
            .push(st.stop_sequence);
    }
    let dups: Vec<String> = seq_by_trip
        .iter()
        .filter_map(|(trip_id, seqs)| {
            let mut sorted = seqs.clone();
            sorted.sort_unstable();
            let before = sorted.len();
            sorted.dedup();
            (sorted.len() < before).then(|| trip_id.to_string())
        })
        .collect();
    issue_from_samples(
        dups,
        Severity::Error,
        Category::ReferentialIntegrity,
        "Trips with duplicate stop_sequence values",
    )
}

fn check_route_colors(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let bad: Vec<String> = gtfs
        .routes
        .values()
        .filter(|r| {
            !r.route_color.is_empty()
                && (r.route_color.len() != 6
                    || !r.route_color.chars().all(|c| c.is_ascii_hexdigit()))
        })
        .map(|r| format!("{} color={}", r.route_id, r.route_color))
        .collect();
    issue_from_samples(
        bad,
        Severity::Warning,
        Category::Display,
        "Routes with invalid hex color",
    )
}

fn check_empty_headsigns(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let count = gtfs
        .trips
        .values()
        .filter(|t| t.trip_headsign.trim().is_empty())
        .count();
    if count > 0 {
        vec![ValidationIssue {
            severity: Severity::Info,
            category: Category::Display,
            message: "Trips with empty trip_headsign".into(),
            count,
            samples: vec![],
        }]
    } else {
        vec![]
    }
}

fn check_unparseable_times(gtfs: &GtfsData) -> Vec<ValidationIssue> {
    let bad: Vec<String> = gtfs
        .stop_times
        .iter()
        .filter(|st| {
            crate::gtfs::parse_time(&st.arrival_time).is_none()
                || crate::gtfs::parse_time(&st.departure_time).is_none()
        })
        .map(|st| {
            format!(
                "trip={} seq={} arr={} dep={}",
                st.trip_id, st.stop_sequence, st.arrival_time, st.departure_time
            )
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    issue_from_samples(
        bad,
        Severity::Error,
        Category::ReferentialIntegrity,
        "Stop times with unparseable arrival/departure time",
    )
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Run all GTFS validation checks against the raw GTFS data.
fn validate_gtfs(gtfs: &GtfsData) -> ValidateResponse {
    // Each checker returns zero or more issues for one validation concern.
    let checkers: Vec<fn(&GtfsData) -> Vec<ValidationIssue>> = vec![
        check_stop_times_trips,
        check_stop_times_stops,
        check_trips_routes,
        check_trips_calendars,
        check_calendar_coverage,
        check_calendar_dates,
        check_coordinates,
        check_stop_names,
        check_parent_stations,
        check_transfer_stops,
        check_transfer_times,
        check_pathway_stops,
        check_pathway_times,
        check_isolated_siblings,
        check_ungrouped_stops,
        check_duplicate_sequences,
        check_route_colors,
        check_empty_headsigns,
        check_unparseable_times,
    ];

    let total_checks = checkers.len();
    let mut issues: Vec<ValidationIssue> = checkers.iter().flat_map(|check| check(gtfs)).collect();

    // Sort: errors first, then warnings, then infos
    issues.sort_by(|a, b| a.severity.cmp(&b.severity));

    let summary = ValidationSummary {
        errors: issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count(),
        warnings: issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count(),
        infos: issues
            .iter()
            .filter(|i| i.severity == Severity::Info)
            .count(),
        total_checks: total_checks as usize,
    };

    ValidateResponse { summary, issues }
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

/// Validate GTFS data by loading it from disk and running all checks.
#[utoipa::path(
    get,
    path = "/api/gtfs/validate",
    responses(
        (status = 200, description = "GTFS validation results", body = ValidateResponse),
        (status = 500, description = "Failed to load GTFS data"),
    ),
    tag = "GTFS"
)]
#[get("/api/gtfs/validate")]
pub async fn get_validate(config: web::Data<AppConfig>) -> HttpResponse {
    let data_dir = config.data.gtfs_dir();

    let result = web::block(move || {
        let data_path = std::path::Path::new(&data_dir);
        let gtfs = GtfsData::load(data_path).map_err(|e| e.to_string())?;
        Ok::<_, String>(validate_gtfs(&gtfs))
    })
    .await;

    match result {
        Ok(Ok(validation)) => HttpResponse::Ok().json(validation),
        Ok(Err(e)) => {
            tracing::error!("GTFS validation failed to load data: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "load_failed", "message": e }
            }))
        }
        Err(e) => {
            tracing::error!("GTFS validation task panicked: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "validation_panic", "message": "Internal error during validation" }
            }))
        }
    }
}

/// Hot-reload GTFS data without downtime.
///
/// Spawns the reload on a blocking thread pool via [`web::block`].
/// The old data continues serving requests until the new RAPTOR index
/// is atomically swapped in via [`ArcSwap::store`].
#[utoipa::path(
    post,
    path = "/api/gtfs/reload",
    responses(
        (status = 200, description = "GTFS data reloaded successfully", body = ReloadResponse),
        (status = 401, description = "Invalid or missing API key"),
        (status = 403, description = "Reload endpoint disabled (no api_key configured)"),
        (status = 500, description = "Reload failed"),
    ),
    security(("api_key" = [])),
    tag = "GTFS"
)]
#[post("/api/gtfs/reload")]
pub async fn post_reload(
    req: actix_web::HttpRequest,
    shared: web::Data<ArcSwap<RaptorData>>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    // --- API key authentication ---
    let expected_key = &config.server.api_key;
    if expected_key.is_empty() {
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": { "id": "disabled", "message": "Reload endpoint is disabled (no api_key configured)" }
        }));
    }
    let provided_key = req
        .headers()
        .get("X-Api-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided_key != expected_key {
        return HttpResponse::Unauthorized().json(serde_json::json!({
            "error": { "id": "unauthorized", "message": "Invalid or missing X-Api-Key header" }
        }));
    }

    let data_dir = config.data.gtfs_dir();
    let raptor_dir = config.data.raptor_dir();
    let transfer_time = config.routing.default_transfer_time;

    let result = web::block(move || {
        let data_path = std::path::Path::new(&data_dir);
        let cache_path = std::path::Path::new(&raptor_dir);
        let gtfs = crate::gtfs::GtfsData::load(data_path).map_err(|e| e.to_string())?;
        let fingerprint = crate::gtfs::gtfs_fingerprint(data_path);
        let new_data = crate::raptor::RaptorData::build(gtfs, transfer_time);
        if let Err(e) = new_data.save(cache_path, &fingerprint) {
            tracing::warn!("Failed to save RAPTOR cache: {e}");
        }
        Ok::<_, String>(Arc::new(new_data))
    })
    .await;

    match result {
        Ok(Ok(new_data)) => {
            let s = &new_data.stats;
            let resp = serde_json::json!({
                "status": "reloaded",
                "loaded_at": s.loaded_at.to_rfc3339(),
                "gtfs": {
                    "agencies": s.agencies,
                    "routes": s.routes,
                    "stops": s.stops,
                    "trips": s.trips,
                    "stop_times": s.stop_times,
                    "calendars": s.calendars,
                    "calendar_dates": s.calendar_dates,
                    "transfers": s.transfers,
                },
                "raptor": {
                    "patterns": s.patterns,
                    "services": s.services,
                }
            });
            shared.store(new_data);
            tracing::info!("GTFS data reloaded");
            HttpResponse::Ok().json(resp)
        }
        Ok(Err(e)) => {
            tracing::error!("Reload failed: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "reload_failed", "message": e }
            }))
        }
        Err(e) => {
            tracing::error!("Reload task panicked: {e}");
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": { "id": "reload_panic", "message": "Internal error during reload" }
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs;
    use rustc_hash::FxHashMap;

    fn make_test_gtfs() -> GtfsData {
        let mut stops = FxHashMap::default();
        stops.insert(
            "S1".into(),
            gtfs::Stop {
                stop_id: "S1".into(),
                stop_name: "A".into(),
                stop_lon: 2.0,
                stop_lat: 48.0,
                parent_station: String::new(),
            },
        );
        let mut routes = FxHashMap::default();
        routes.insert(
            "R1".into(),
            gtfs::Route {
                route_id: "R1".into(),
                agency_id: "A1".into(),
                route_short_name: "1".into(),
                route_long_name: "L".into(),
                route_type: 1,
                route_color: String::new(),
                route_text_color: String::new(),
            },
        );
        let mut trips = FxHashMap::default();
        trips.insert(
            "T1".into(),
            gtfs::Trip {
                route_id: "R1".into(),
                service_id: "SVC1".into(),
                trip_id: "T1".into(),
                trip_headsign: "A".into(),
            },
        );
        let stop_times = vec![gtfs::StopTime {
            trip_id: "T1".into(),
            arrival_time: "08:00:00".into(),
            departure_time: "08:01:00".into(),
            stop_id: "S1".into(),
            stop_sequence: 0,
        }];
        let mut calendars = FxHashMap::default();
        calendars.insert(
            "SVC1".into(),
            gtfs::Calendar {
                service_id: "SVC1".into(),
                monday: 1,
                tuesday: 1,
                wednesday: 1,
                thursday: 1,
                friday: 1,
                saturday: 1,
                sunday: 1,
                start_date: "20260101".into(),
                end_date: "20261231".into(),
            },
        );
        GtfsData {
            agencies: vec![],
            routes,
            stops,
            trips,
            stop_times,
            calendars,
            calendar_dates: vec![],
            transfers: vec![],
            pathways: vec![],
        }
    }

    #[test]
    fn validate_clean_data() {
        let gtfs = make_test_gtfs();
        let result = validate_gtfs(&gtfs);
        assert!(result.summary.total_checks > 0);
        assert_eq!(result.summary.errors, 0);
    }

    #[test]
    fn validate_orphan_stop_time() {
        let mut gtfs = make_test_gtfs();
        gtfs.stop_times.push(gtfs::StopTime {
            trip_id: "NONEXISTENT".into(),
            arrival_time: "09:00:00".into(),
            departure_time: "09:01:00".into(),
            stop_id: "S1".into(),
            stop_sequence: 0,
        });
        let result = validate_gtfs(&gtfs);
        assert!(result.summary.errors > 0);
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.message.contains("non-existent trip_id"))
        );
    }

    #[test]
    fn validate_bad_coordinates() {
        let mut gtfs = make_test_gtfs();
        gtfs.stops.insert(
            "S2".into(),
            gtfs::Stop {
                stop_id: "S2".into(),
                stop_name: "Bad".into(),
                stop_lon: 0.0,
                stop_lat: 0.0,
                parent_station: String::new(),
            },
        );
        let result = validate_gtfs(&gtfs);
        assert!(result.summary.warnings > 0);
        assert!(
            result
                .issues
                .iter()
                .any(|i| i.message.contains("coordinates"))
        );
    }
}
