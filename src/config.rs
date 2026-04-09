//! Application configuration loaded from a YAML file.
//!
//! All fields have sensible defaults so a partial or missing config file
//! still produces a valid configuration.

use serde::Deserialize;
use std::path::Path;
use tracing::info;

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Application configuration, deserialized from `config.yaml`.
#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct AppConfig {
    /// HTTP server settings.
    #[serde(default)]
    pub server: ServerConfig,

    /// Data directories and download URLs.
    #[serde(default)]
    pub data: DataConfig,

    /// Public-transport routing parameters.
    #[serde(default)]
    pub routing: RoutingConfig,

    /// Valhalla routing engine connection.
    #[serde(default)]
    pub valhalla: ValhallaConfig,

    /// Map display defaults.
    #[serde(default)]
    pub map: MapConfig,

    /// Bicycle routing profiles.
    #[serde(default)]
    pub bike: BikeConfig,
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// Address to bind the HTTP server to.
    #[serde(default = "default_bind")]
    pub bind: String,

    /// Port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Number of actix-web worker threads. 0 = auto (one per logical CPU).
    #[serde(default)]
    pub workers: usize,

    /// Minimum log level: trace, debug, info, warn, error.
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Graceful shutdown timeout in seconds. In-flight requests get this
    /// long to complete before the server force-closes connections.
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: u64,

    /// API key required for admin endpoints (e.g. POST /api/reload).
    /// If empty, admin endpoints are disabled.
    #[serde(default)]
    pub api_key: String,

    /// Allowed CORS origins. Empty = no CORS header (same-origin only).
    /// Use `["*"]` to allow all origins (not recommended in production).
    #[serde(default)]
    pub cors_origins: Vec<String>,

    /// Maximum requests per second per IP address (rate limiting).
    /// 0 = no rate limiting.
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            workers: 0,
            log_level: default_log_level(),
            shutdown_timeout: default_shutdown_timeout(),
            api_key: String::new(),
            cors_origins: Vec::new(),
            rate_limit: default_rate_limit(),
        }
    }
}

fn default_bind() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_shutdown_timeout() -> u64 {
    30
}
fn default_rate_limit() -> u32 {
    20
}

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DataConfig {
    /// Root data directory. Sub-directories (gtfs, osm, raptor, ban) are
    /// derived automatically: `{dir}/gtfs`, `{dir}/osm`, etc.
    #[serde(default = "default_data_dir")]
    pub dir: String,

    /// Download URL for GTFS data.
    #[serde(default)]
    pub gtfs_url: String,

    /// Download URL for OSM data.
    #[serde(default)]
    pub osm_url: String,

    /// List of covered department codes.
    #[serde(default)]
    pub departments: Vec<String>,
}

#[allow(dead_code)]
impl DataConfig {
    pub fn gtfs_dir(&self) -> String {
        format!("{}/gtfs", self.dir)
    }
    pub fn osm_dir(&self) -> String {
        format!("{}/osm", self.dir)
    }
    pub fn raptor_dir(&self) -> String {
        format!("{}/raptor", self.dir)
    }
    pub fn ban_dir(&self) -> String {
        format!("{}/ban", self.dir)
    }
    pub fn tiles_dir(&self) -> String {
        format!("{}/tiles", self.dir)
    }
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            dir: default_data_dir(),
            gtfs_url: String::new(),
            osm_url: String::new(),
            departments: Vec::new(),
        }
    }
}

fn default_data_dir() -> String {
    "data".to_string()
}

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RoutingConfig {
    /// Maximum number of alternative journeys returned per request.
    #[serde(default = "default_max_journeys")]
    pub max_journeys: usize,

    /// Maximum number of transfers (vehicle changes) allowed in a journey.
    #[serde(default = "default_max_transfers")]
    pub max_transfers: usize,

    /// Default walking time (seconds) for transfers not specified in GTFS.
    #[serde(default = "default_transfer_time")]
    pub default_transfer_time: u32,

    /// Maximum journey duration in seconds.
    #[serde(default = "default_max_duration")]
    pub max_duration: u32,

    /// Maximum walking distance to reach the nearest stop (meters).
    /// Coordinates beyond this radius will be rejected.
    /// Default: 1500 m (~20 min at 5 km/h).
    #[serde(default = "default_max_nearest_stop_distance")]
    pub max_nearest_stop_distance: u32,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            max_journeys: default_max_journeys(),
            max_transfers: default_max_transfers(),
            default_transfer_time: default_transfer_time(),
            max_duration: default_max_duration(),
            max_nearest_stop_distance: default_max_nearest_stop_distance(),
        }
    }
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
fn default_max_nearest_stop_distance() -> u32 {
    1500
}

// ---------------------------------------------------------------------------
// Valhalla
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ValhallaConfig {
    /// Valhalla routing engine hostname.
    #[serde(default = "default_valhalla_host")]
    pub host: String,

    /// Valhalla routing engine port.
    #[serde(default = "default_valhalla_port")]
    pub port: u16,
}

impl Default for ValhallaConfig {
    fn default() -> Self {
        Self {
            host: default_valhalla_host(),
            port: default_valhalla_port(),
        }
    }
}

fn default_valhalla_host() -> String {
    "localhost".to_string()
}
fn default_valhalla_port() -> u16 {
    8002
}

// ---------------------------------------------------------------------------
// Map
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MapConfig {
    /// Map center latitude.
    #[serde(default = "default_map_center_lat")]
    pub center_lat: f64,

    /// Map center longitude.
    #[serde(default = "default_map_center_lon")]
    pub center_lon: f64,

    /// Map default zoom level.
    #[serde(default = "default_map_zoom")]
    pub zoom: u8,

    /// South-west corner latitude of the map bounds.
    #[serde(default = "default_bounds_sw_lat")]
    pub bounds_sw_lat: f64,

    /// South-west corner longitude of the map bounds.
    #[serde(default = "default_bounds_sw_lon")]
    pub bounds_sw_lon: f64,

    /// North-east corner latitude of the map bounds.
    #[serde(default = "default_bounds_ne_lat")]
    pub bounds_ne_lat: f64,

    /// North-east corner longitude of the map bounds.
    #[serde(default = "default_bounds_ne_lon")]
    pub bounds_ne_lon: f64,

    /// Upstream tile server URL template for tile caching proxy.
    /// Placeholders: `{s}` (subdomain), `{z}`, `{x}`, `{y}`, `{r}` (retina).
    #[serde(default = "default_tile_url")]
    pub tile_url: String,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            center_lat: default_map_center_lat(),
            center_lon: default_map_center_lon(),
            zoom: default_map_zoom(),
            bounds_sw_lat: default_bounds_sw_lat(),
            bounds_sw_lon: default_bounds_sw_lon(),
            bounds_ne_lat: default_bounds_ne_lat(),
            bounds_ne_lon: default_bounds_ne_lon(),
            tile_url: default_tile_url(),
        }
    }
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
// Île-de-France bounding box
fn default_bounds_sw_lat() -> f64 {
    48.1
}
fn default_bounds_sw_lon() -> f64 {
    1.4
}
fn default_bounds_ne_lat() -> f64 {
    49.3
}
fn default_bounds_ne_lon() -> f64 {
    3.6
}
fn default_tile_url() -> String {
    "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png".to_string()
}

// ---------------------------------------------------------------------------
// Bike profiles
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BikeConfig {
    /// City bike profile (e.g. Velib' mécanique).
    #[serde(default = "default_bike_city")]
    pub city: BikeProfile,

    /// E-bike profile (e.g. Velib' électrique / VAE).
    #[serde(default = "default_bike_ebike")]
    pub ebike: BikeProfile,

    /// Road bike profile (fast commuter / road cycling).
    #[serde(default = "default_bike_road")]
    pub road: BikeProfile,
}

impl Default for BikeConfig {
    fn default() -> Self {
        Self {
            city: default_bike_city(),
            ebike: default_bike_ebike(),
            road: default_bike_road(),
        }
    }
}

/// Configuration for a Valhalla bicycle costing profile.
#[derive(Debug, Deserialize, Clone)]
pub struct BikeProfile {
    /// Cycling speed in km/h.
    #[serde(default = "default_cycling_speed")]
    pub cycling_speed: f64,
    /// Road preference (0.0 = avoid roads, 1.0 = prefer roads).
    #[serde(default = "default_use_roads")]
    pub use_roads: f64,
    /// Hill preference (0.0 = avoid hills, 1.0 = prefer hills).
    #[serde(default = "default_use_hills")]
    pub use_hills: f64,
    /// Valhalla bicycle type: City, Hybrid, Road, Cross, Mountain.
    #[serde(default = "default_bicycle_type")]
    pub bicycle_type: String,
}

fn default_cycling_speed() -> f64 {
    20.0
}
fn default_use_roads() -> f64 {
    0.5
}
fn default_use_hills() -> f64 {
    0.5
}
fn default_bicycle_type() -> String {
    "Hybrid".to_string()
}

fn default_bike_city() -> BikeProfile {
    BikeProfile {
        cycling_speed: 16.0,
        use_roads: 0.2,
        use_hills: 0.3,
        bicycle_type: "City".to_string(),
    }
}
fn default_bike_ebike() -> BikeProfile {
    BikeProfile {
        cycling_speed: 21.0,
        use_roads: 0.4,
        use_hills: 0.8,
        bicycle_type: "Hybrid".to_string(),
    }
}
fn default_bike_road() -> BikeProfile {
    BikeProfile {
        cycling_speed: 25.0,
        use_roads: 0.6,
        use_hills: 0.5,
        bicycle_type: "Road".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_returns_expected_values() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.server.bind, "0.0.0.0");
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.server.workers, 0);
        assert_eq!(cfg.server.log_level, "info");
        assert_eq!(cfg.data.dir, "data");
        assert_eq!(cfg.data.gtfs_dir(), "data/gtfs");
        assert_eq!(cfg.data.ban_dir(), "data/ban");
        assert_eq!(cfg.routing.max_journeys, 5);
        assert_eq!(cfg.routing.max_transfers, 5);
        assert_eq!(cfg.routing.default_transfer_time, 120);
        assert_eq!(cfg.routing.max_duration, 10800);
        assert_eq!(cfg.valhalla.host, "localhost");
        assert_eq!(cfg.valhalla.port, 8002);
        assert!((cfg.map.center_lat - 48.8566).abs() < 1e-6);
        assert!((cfg.map.center_lon - 2.3522).abs() < 1e-6);
        assert_eq!(cfg.map.zoom, 11);
        assert!((cfg.bike.city.cycling_speed - 16.0).abs() < 1e-6);
        assert!((cfg.bike.ebike.cycling_speed - 21.0).abs() < 1e-6);
        assert!((cfg.bike.road.cycling_speed - 25.0).abs() < 1e-6);
    }

    #[test]
    fn load_nonexistent_file_returns_defaults() {
        let path = Path::new("/tmp/glove_test_nonexistent_config.yaml");
        let _ = std::fs::remove_file(path);
        let cfg = AppConfig::load(path);
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.routing.max_journeys, 5);
        assert_eq!(cfg.valhalla.host, "localhost");
    }

    #[test]
    fn load_valid_yaml_overrides_nested_fields() {
        let path = Path::new("/tmp/glove_test_nested_config.yaml");
        let yaml = r#"
server:
  bind: "127.0.0.1"
  port: 9090
  workers: 4
  log_level: "debug"

data:
  dir: "custom"

routing:
  max_journeys: 10
  max_transfers: 3
  default_transfer_time: 60
  max_duration: 7200

valhalla:
  host: "valhalla.local"
  port: 8003

map:
  center_lat: 43.2965
  center_lon: 5.3698
  zoom: 13

bike:
  city:
    cycling_speed: 14.0
    use_roads: 0.1
"#;
        std::fs::write(path, yaml).unwrap();
        let cfg = AppConfig::load(path);

        assert_eq!(cfg.server.bind, "127.0.0.1");
        assert_eq!(cfg.server.port, 9090);
        assert_eq!(cfg.server.workers, 4);
        assert_eq!(cfg.server.log_level, "debug");
        assert_eq!(cfg.data.dir, "custom");
        assert_eq!(cfg.data.gtfs_dir(), "custom/gtfs");
        assert_eq!(cfg.data.ban_dir(), "custom/ban");
        assert_eq!(cfg.routing.max_journeys, 10);
        assert_eq!(cfg.routing.max_transfers, 3);
        assert_eq!(cfg.routing.default_transfer_time, 60);
        assert_eq!(cfg.routing.max_duration, 7200);
        assert_eq!(cfg.valhalla.host, "valhalla.local");
        assert_eq!(cfg.valhalla.port, 8003);
        assert!((cfg.map.center_lat - 43.2965).abs() < 1e-6);
        assert_eq!(cfg.map.zoom, 13);
        assert!((cfg.bike.city.cycling_speed - 14.0).abs() < 1e-6);
        // ebike/road should keep defaults
        assert!((cfg.bike.ebike.cycling_speed - 21.0).abs() < 1e-6);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_partial_yaml_fills_defaults() {
        let path = Path::new("/tmp/glove_test_partial_nested.yaml");
        let yaml = r#"
server:
  port: 7070
routing:
  max_journeys: 8
"#;
        std::fs::write(path, yaml).unwrap();
        let cfg = AppConfig::load(path);

        assert_eq!(cfg.server.port, 7070);
        assert_eq!(cfg.server.bind, "0.0.0.0"); // default
        assert_eq!(cfg.routing.max_journeys, 8);
        assert_eq!(cfg.routing.max_transfers, 5); // default
        assert_eq!(cfg.valhalla.host, "localhost"); // whole section default

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn data_config_derived_dirs() {
        let data = DataConfig {
            dir: "mydata".to_string(),
            ..DataConfig::default()
        };
        assert_eq!(data.gtfs_dir(), "mydata/gtfs");
        assert_eq!(data.osm_dir(), "mydata/osm");
        assert_eq!(data.raptor_dir(), "mydata/raptor");
        assert_eq!(data.ban_dir(), "mydata/ban");
    }

    #[test]
    fn bike_profile_defaults() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.bike.city.bicycle_type, "City");
        assert!((cfg.bike.city.use_roads - 0.2).abs() < 1e-6);
        assert!((cfg.bike.city.use_hills - 0.3).abs() < 1e-6);
        assert_eq!(cfg.bike.ebike.bicycle_type, "Hybrid");
        assert!((cfg.bike.ebike.use_roads - 0.4).abs() < 1e-6);
        assert!((cfg.bike.ebike.use_hills - 0.8).abs() < 1e-6);
        assert_eq!(cfg.bike.road.bicycle_type, "Road");
        assert!((cfg.bike.road.use_roads - 0.6).abs() < 1e-6);
        assert!((cfg.bike.road.use_hills - 0.5).abs() < 1e-6);
    }

    #[test]
    fn map_config_defaults() {
        let cfg = AppConfig::default();
        assert!((cfg.map.bounds_sw_lat - 48.1).abs() < 1e-6);
        assert!((cfg.map.bounds_sw_lon - 1.4).abs() < 1e-6);
        assert!((cfg.map.bounds_ne_lat - 49.3).abs() < 1e-6);
        assert!((cfg.map.bounds_ne_lon - 3.6).abs() < 1e-6);
    }

    #[test]
    fn load_yaml_with_bike_profiles() {
        let path = Path::new("/tmp/glove_test_bike_config.yaml");
        let yaml = r#"
bike:
  city:
    cycling_speed: 12.0
    use_roads: 0.1
    use_hills: 0.2
    bicycle_type: "Mountain"
  ebike:
    cycling_speed: 18.0
  road:
    cycling_speed: 30.0
    bicycle_type: "Road"
"#;
        std::fs::write(path, yaml).unwrap();
        let cfg = AppConfig::load(path);
        assert!((cfg.bike.city.cycling_speed - 12.0).abs() < 1e-6);
        assert_eq!(cfg.bike.city.bicycle_type, "Mountain");
        assert!((cfg.bike.ebike.cycling_speed - 18.0).abs() < 1e-6);
        // ebike defaults for unspecified fields
        assert!((cfg.bike.ebike.use_roads - 0.5).abs() < 1e-6);
        assert!((cfg.bike.road.cycling_speed - 30.0).abs() < 1e-6);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_yaml_with_map_bounds() {
        let path = Path::new("/tmp/glove_test_map_bounds.yaml");
        let yaml = r#"
map:
  center_lat: 43.0
  center_lon: 5.0
  zoom: 13
  bounds_sw_lat: 42.0
  bounds_sw_lon: 4.0
  bounds_ne_lat: 44.0
  bounds_ne_lon: 6.0
"#;
        std::fs::write(path, yaml).unwrap();
        let cfg = AppConfig::load(path);
        assert!((cfg.map.center_lat - 43.0).abs() < 1e-6);
        assert_eq!(cfg.map.zoom, 13);
        assert!((cfg.map.bounds_sw_lat - 42.0).abs() < 1e-6);
        assert!((cfg.map.bounds_ne_lon - 6.0).abs() < 1e-6);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn server_config_defaults() {
        let s = ServerConfig::default();
        assert_eq!(s.bind, "0.0.0.0");
        assert_eq!(s.port, 8080);
        assert_eq!(s.workers, 0);
        assert_eq!(s.log_level, "info");
    }

    #[test]
    fn routing_config_defaults() {
        let r = RoutingConfig::default();
        assert_eq!(r.max_journeys, 5);
        assert_eq!(r.max_transfers, 5);
        assert_eq!(r.default_transfer_time, 120);
        assert_eq!(r.max_duration, 10800);
    }

    #[test]
    fn valhalla_config_defaults() {
        let v = ValhallaConfig::default();
        assert_eq!(v.host, "localhost");
        assert_eq!(v.port, 8002);
    }
}
