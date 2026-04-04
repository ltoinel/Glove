//! GTFS data model and CSV loader.
//!
//! Reads the standard GTFS text files (agency.txt, routes.txt, stops.txt, etc.)
//! into in-memory structures. Each file is parsed with flexible CSV handling
//! to tolerate minor format variations in real-world feeds.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

/// A transit agency (operator).
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Agency {
    pub agency_id: String,
    pub agency_name: String,
    agency_url: String,
    agency_timezone: String,
}

/// A transit route (line), e.g. "Metro 1" or "Bus 72".
#[derive(Debug, Serialize, Deserialize)]
pub struct Route {
    pub route_id: String,
    #[allow(dead_code)]
    pub agency_id: String,
    /// Short display name (e.g. "1", "A", "72").
    pub route_short_name: String,
    #[allow(dead_code)]
    pub route_long_name: String,
    /// GTFS route type: 0=tram, 1=metro, 2=rail, 3=bus, 7=funicular.
    pub route_type: u16,
    /// Hex color for the route line (e.g. "EB2132").
    #[serde(default)]
    pub route_color: String,
    /// Hex color for text on the route badge.
    #[serde(default)]
    pub route_text_color: String,
}

/// A physical stop point where passengers board/alight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stop {
    pub stop_id: String,
    #[serde(default)]
    pub stop_name: String,
    #[serde(default)]
    pub stop_lon: f64,
    #[serde(default)]
    pub stop_lat: f64,
    /// Parent station ID. Stops sharing the same parent are considered
    /// transferable with a default walking time.
    #[serde(default)]
    pub parent_station: String,
}

/// A single vehicle trip on a route.
#[derive(Debug, Serialize, Deserialize)]
pub struct Trip {
    pub route_id: String,
    pub service_id: String,
    pub trip_id: String,
    /// Destination sign displayed on the vehicle.
    #[serde(default)]
    pub trip_headsign: String,
}

/// A scheduled arrival/departure at a stop within a trip.
#[derive(Debug, Serialize, Deserialize)]
pub struct StopTime {
    pub trip_id: String,
    /// Arrival time in HH:MM:SS format (may exceed 24:00:00).
    pub arrival_time: String,
    /// Departure time in HH:MM:SS format.
    pub departure_time: String,
    pub stop_id: String,
    /// Position of this stop in the trip sequence (0-based).
    pub stop_sequence: u32,
}

/// Weekly service pattern with validity period.
#[derive(Debug, Serialize, Deserialize)]
pub struct Calendar {
    pub service_id: String,
    pub monday: u8,
    pub tuesday: u8,
    pub wednesday: u8,
    pub thursday: u8,
    pub friday: u8,
    pub saturday: u8,
    pub sunday: u8,
    /// Start of validity in YYYYMMDD format.
    pub start_date: String,
    /// End of validity in YYYYMMDD format.
    pub end_date: String,
}

/// Exception to the regular calendar (added or removed service on a date).
#[derive(Debug, Serialize, Deserialize)]
pub struct CalendarDate {
    pub service_id: String,
    /// Date in YYYYMMDD format.
    pub date: String,
    /// 1 = service added, 2 = service removed.
    pub exception_type: u8,
}

/// A possible transfer between two stops (walking connection).
#[derive(Debug, Serialize, Deserialize)]
pub struct Transfer {
    pub from_stop_id: String,
    pub to_stop_id: String,
    /// Minimum transfer time in seconds.
    #[serde(default)]
    pub min_transfer_time: Option<u32>,
}

/// Container for all raw GTFS data loaded from CSV files.
pub struct GtfsData {
    pub agencies: Vec<Agency>,
    pub routes: HashMap<String, Route>,
    pub stops: HashMap<String, Stop>,
    pub trips: HashMap<String, Trip>,
    pub stop_times: Vec<StopTime>,
    pub calendars: HashMap<String, Calendar>,
    pub calendar_dates: Vec<CalendarDate>,
    pub transfers: Vec<Transfer>,
}

/// Load a CSV file into a vector of deserialized records.
/// Malformed rows are skipped and counted.
fn load_csv<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Vec<T>, Box<dyn std::error::Error>> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_path(path)?;
    let mut records = Vec::new();
    let mut skipped = 0u64;
    for result in reader.deserialize() {
        match result {
            Ok(record) => records.push(record),
            Err(_) => skipped += 1,
        }
    }
    if skipped > 0 {
        warn!("{}: skipped {} malformed rows", path.display(), skipped);
    }
    Ok(records)
}

impl GtfsData {
    /// Load all GTFS files from the given directory.
    ///
    /// Expected files: agency.txt, routes.txt, stops.txt, trips.txt,
    /// stop_times.txt, calendar.txt, calendar_dates.txt, transfers.txt.
    pub fn load(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Loading GTFS data from {}", data_dir.display());

        let agencies: Vec<Agency> = load_csv(&data_dir.join("agency.txt"))?;
        info!("{} agencies", agencies.len());

        let routes_vec: Vec<Route> = load_csv(&data_dir.join("routes.txt"))?;
        info!("{} routes", routes_vec.len());
        let routes = routes_vec
            .into_iter()
            .map(|r| (r.route_id.clone(), r))
            .collect();

        let stops_vec: Vec<Stop> = load_csv(&data_dir.join("stops.txt"))?;
        info!("{} stops", stops_vec.len());
        let stops = stops_vec
            .into_iter()
            .map(|s| (s.stop_id.clone(), s))
            .collect();

        let trips_vec: Vec<Trip> = load_csv(&data_dir.join("trips.txt"))?;
        info!("{} trips", trips_vec.len());
        let trips = trips_vec
            .into_iter()
            .map(|t| (t.trip_id.clone(), t))
            .collect();

        info!("Loading stop_times...");
        let stop_times: Vec<StopTime> = load_csv(&data_dir.join("stop_times.txt"))?;
        info!("{} stop_times", stop_times.len());

        let calendars_vec: Vec<Calendar> = load_csv(&data_dir.join("calendar.txt"))?;
        info!("{} calendars", calendars_vec.len());
        let calendars = calendars_vec
            .into_iter()
            .map(|c| (c.service_id.clone(), c))
            .collect();

        let calendar_dates: Vec<CalendarDate> = load_csv(&data_dir.join("calendar_dates.txt"))?;
        info!("{} calendar_dates", calendar_dates.len());

        let transfers: Vec<Transfer> = load_csv(&data_dir.join("transfers.txt"))?;
        info!("{} transfers", transfers.len());

        info!("GTFS data loaded");

        Ok(GtfsData {
            agencies,
            routes,
            stops,
            trips,
            stop_times,
            calendars,
            calendar_dates,
            transfers,
        })
    }
}

/// Parse a GTFS time string ("HH:MM:SS") into seconds since midnight.
///
/// GTFS allows times beyond 24:00:00 for trips crossing midnight
/// (e.g. "25:30:00" = 1:30 AM the next day).
pub fn parse_time(time_str: &str) -> Option<u32> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let s: u32 = parts[2].parse().ok()?;
    Some(h * 3600 + m * 60 + s)
}

/// Compute a SHA-256 fingerprint of the GTFS directory based on file sizes.
///
/// This is a fast way to detect changes without reading file contents.
/// Returns a hex-encoded hash string.
pub fn gtfs_fingerprint(data_dir: &Path) -> String {
    let files = [
        "agency.txt",
        "routes.txt",
        "stops.txt",
        "trips.txt",
        "stop_times.txt",
        "calendar.txt",
        "calendar_dates.txt",
        "transfers.txt",
    ];
    let mut hasher = Sha256::new();
    for name in &files {
        let path = data_dir.join(name);
        if let Ok(meta) = std::fs::metadata(&path) {
            hasher.update(name.as_bytes());
            hasher.update(meta.len().to_le_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}
