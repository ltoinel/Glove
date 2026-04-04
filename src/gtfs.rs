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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // --- parse_time ---

    #[test]
    fn parse_time_normal() {
        assert_eq!(parse_time("08:30:00"), Some(30600));
    }

    #[test]
    fn parse_time_midnight() {
        assert_eq!(parse_time("00:00:00"), Some(0));
    }

    #[test]
    fn parse_time_beyond_24h() {
        // GTFS allows times past midnight of the next service day.
        // 25*3600 + 30*60 + 0 = 91800
        assert_eq!(parse_time("25:30:00"), Some(91800));
    }

    #[test]
    fn parse_time_edge_end_of_day() {
        // 23*3600 + 59*60 + 59 = 86399
        assert_eq!(parse_time("23:59:59"), Some(86399));
    }

    #[test]
    fn parse_time_missing_seconds() {
        // Only 2 parts — not a valid GTFS time.
        assert_eq!(parse_time("8:30"), None);
    }

    #[test]
    fn parse_time_alpha() {
        assert_eq!(parse_time("abc"), None);
    }

    #[test]
    fn parse_time_empty() {
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn parse_time_two_parts_only() {
        assert_eq!(parse_time("08:30"), None);
    }

    #[test]
    fn parse_time_four_parts() {
        assert_eq!(parse_time("08:30:00:00"), None);
    }

    #[test]
    fn parse_time_non_numeric_parts() {
        assert_eq!(parse_time("ab:cd:ef"), None);
    }

    // --- gtfs_fingerprint ---

    #[test]
    fn gtfs_fingerprint_is_hex_64_chars() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let fp = gtfs_fingerprint(dir.path());
        // SHA-256 produces a 32-byte digest → 64 hex characters.
        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn gtfs_fingerprint_same_dir_is_deterministic() {
        let dir = TempDir::new().expect("failed to create temp dir");
        // Write a dummy GTFS file so the hasher has something to consume.
        let stops_path = dir.path().join("stops.txt");
        let mut f = std::fs::File::create(&stops_path).expect("create stops.txt");
        writeln!(f, "stop_id,stop_name").expect("write header");

        let fp1 = gtfs_fingerprint(dir.path());
        let fp2 = gtfs_fingerprint(dir.path());
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn gtfs_fingerprint_nonexistent_dir_is_deterministic() {
        let missing = Path::new("/tmp/__glove_nonexistent_dir_for_test__");
        // No files match, so the hasher finalises over an empty input both times.
        let fp1 = gtfs_fingerprint(missing);
        let fp2 = gtfs_fingerprint(missing);
        assert_eq!(fp1, fp2);
        // Still a valid 64-char hex string.
        assert_eq!(fp1.len(), 64);
    }

    #[test]
    fn gtfs_fingerprint_changes_when_file_size_changes() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let stops_path = dir.path().join("stops.txt");

        // Write a small file.
        {
            let mut f = std::fs::File::create(&stops_path).expect("create stops.txt");
            writeln!(f, "stop_id,stop_name").expect("write header");
        }
        let fp_small = gtfs_fingerprint(dir.path());

        // Append more content so the file size changes.
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&stops_path)
                .expect("open stops.txt for append");
            writeln!(f, "S1,Central Station").expect("append row");
        }
        let fp_large = gtfs_fingerprint(dir.path());

        assert_ne!(fp_small, fp_large);
    }

    // --- GtfsData::load ---

    #[test]
    fn gtfs_load_nonexistent_dir_returns_error() {
        let missing = Path::new("/tmp/__glove_nonexistent_gtfs_dir__");
        let result = GtfsData::load(missing);
        assert!(
            result.is_err(),
            "expected an error when loading from a non-existent directory"
        );
    }
}
