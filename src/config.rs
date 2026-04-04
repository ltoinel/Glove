//! Application configuration loaded from a YAML file.
//!
//! All fields have sensible defaults so a partial or missing config file
//! still produces a valid configuration.

use serde::Deserialize;
use std::path::Path;
use tracing::info;

/// Application configuration, deserialized from `config.yaml`.
#[derive(Debug, Deserialize)]
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

    /// Number of actix-web worker threads. 0 = auto (one per logical CPU).
    #[serde(default = "default_workers")]
    pub workers: usize,

    /// Minimum log level: trace, debug, info, warn, error.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_bind() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_data_dir() -> String {
    "data".to_string()
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
fn default_workers() -> usize {
    0
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            data_dir: default_data_dir(),
            max_journeys: default_max_journeys(),
            max_transfers: default_max_transfers(),
            default_transfer_time: default_transfer_time(),
            max_duration: default_max_duration(),
            workers: default_workers(),
            log_level: default_log_level(),
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
