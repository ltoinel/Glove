//! Application configuration loaded from a YAML file.
//!
//! All fields have sensible defaults so a partial or missing config file
//! still produces a valid configuration.

use serde::Deserialize;
use std::path::Path;
use tracing::info;

/// Application configuration, deserialized from `config.yaml`.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct AppConfig {
    /// Address to bind the HTTP server to.
    #[serde(default = "default_bind")]
    pub bind: String,

    /// Port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Path to the directory containing GTFS CSV files.
    #[serde(default = "default_data_dir")]
    pub data_dir: String,

    /// Path to the directory containing OSM PBF files.
    #[serde(default = "default_osm_dir")]
    pub osm_dir: String,

    /// Path to the directory for cached RAPTOR index.
    #[serde(default = "default_raptor_dir")]
    pub raptor_dir: String,

    /// Path to the directory containing BAN address CSV files.
    #[serde(default = "default_ban_dir")]
    pub ban_dir: String,

    /// Download URL for GTFS data.
    #[serde(default)]
    pub gtfs_url: String,

    /// Download URL for OSM data.
    #[serde(default)]
    pub osm_url: String,

    /// List of covered department codes.
    #[serde(default = "default_departments")]
    pub departments: Vec<String>,

    /// Maximum number of alternative journeys returned per request.
    #[serde(default = "default_max_journeys")]
    pub max_journeys: usize,

    /// Maximum number of transfers (vehicle changes) allowed in a journey.
    #[serde(default = "default_max_transfers")]
    pub max_transfers: usize,

    /// Default walking time (seconds) for transfers not specified in GTFS.
    #[serde(default = "default_transfer_time")]
    pub default_transfer_time: u32,

    /// Maximum journey duration in seconds. Journeys exceeding this are discarded.
    #[serde(default = "default_max_duration")]
    pub max_duration: u32,

    /// Valhalla routing engine hostname.
    #[serde(default = "default_valhalla_host")]
    pub valhalla_host: String,

    /// Valhalla routing engine port.
    #[serde(default = "default_valhalla_port")]
    pub valhalla_port: u16,

    /// Number of actix-web worker threads. 0 = auto (one per logical CPU).
    #[serde(default = "default_workers")]
    pub workers: usize,

    /// Minimum log level: trace, debug, info, warn, error.
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Map center latitude.
    #[serde(default = "default_map_center_lat")]
    pub map_center_lat: f64,

    /// Map center longitude.
    #[serde(default = "default_map_center_lon")]
    pub map_center_lon: f64,

    /// Map default zoom level.
    #[serde(default = "default_map_zoom")]
    pub map_zoom: u8,
}

fn default_bind() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_data_dir() -> String {
    "data/gtfs".to_string()
}
fn default_osm_dir() -> String {
    "data/osm".to_string()
}
fn default_raptor_dir() -> String {
    "data/raptor".to_string()
}
fn default_ban_dir() -> String {
    "data/ban".to_string()
}
fn default_departments() -> Vec<String> {
    Vec::new()
}
fn default_max_journeys() -> usize {
    5
}
fn default_max_transfers() -> usize {
    5
}
fn default_transfer_time() -> u32 {
    120
}
fn default_max_duration() -> u32 {
    10800
}
fn default_valhalla_host() -> String {
    "localhost".to_string()
}
fn default_valhalla_port() -> u16 {
    8002
}
fn default_workers() -> usize {
    0
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_map_center_lat() -> f64 {
    48.8566
}
fn default_map_center_lon() -> f64 {
    2.3522
}
fn default_map_zoom() -> u8 {
    11
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            data_dir: default_data_dir(),
            osm_dir: default_osm_dir(),
            raptor_dir: default_raptor_dir(),
            ban_dir: default_ban_dir(),
            gtfs_url: String::new(),
            osm_url: String::new(),
            departments: default_departments(),
            max_journeys: default_max_journeys(),
            max_transfers: default_max_transfers(),
            default_transfer_time: default_transfer_time(),
            max_duration: default_max_duration(),
            valhalla_host: default_valhalla_host(),
            valhalla_port: default_valhalla_port(),
            workers: default_workers(),
            log_level: default_log_level(),
            map_center_lat: default_map_center_lat(),
            map_center_lon: default_map_center_lon(),
            map_zoom: default_map_zoom(),
        }
    }
}

impl AppConfig {
    /// Load configuration from a YAML file. Falls back to defaults if the
    /// file does not exist. Exits the process on parse errors.
    pub fn load(path: &Path) -> Self {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match serde_yaml::from_str(&content) {
                    Ok(config) => {
                        info!("Configuration loaded from {}", path.display());
                        return config;
                    }
                    Err(e) => {
                        eprintln!("Error parsing {}: {e}", path.display());
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Error reading {}: {e}", path.display());
                    std::process::exit(1);
                }
            }
        }

        info!("No config file found at {}, using defaults", path.display());
        Self::default()
    }
}
