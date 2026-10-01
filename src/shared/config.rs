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

    /// Default pedestrian walking profile (first/last mile, walk directions).
    #[serde(default)]
    pub pedestrian: PedestrianConfig,

    /// Wheelchair accessibility routing options.
    #[serde(default)]
    pub wheelchair: WheelchairConfig,

    /// Real-time road traffic overlay (Sytadin / DiRIF).
    #[serde(default)]
    pub traffic: TrafficConfig,

    /// Real-time transit feeds (GTFS-Realtime, SIRI).
    #[serde(default)]
    pub realtime: RealtimeConfig,
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
    pub fn sytadin_dir(&self) -> String {
        format!("{}/sytadin", self.dir)
    }
    /// Operator-authored disruption catalog. A file rather than a directory:
    /// it is one JSON document, rewritten whole on every change.
    pub fn disruptions_file(&self) -> String {
        format!("{}/disruptions/disruptions.json", self.dir)
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

    /// Maximum number of transfers (vehicle changes) allowed in a journey. Each
    /// one is a RAPTOR round, and the per-thread search tables grow with it
    /// (`(max_transfers + 2) × stops` entries each).
    #[serde(default = "default_max_transfers")]
    pub max_transfers: usize,

    /// Default walking time (seconds) for transfers not specified in GTFS.
    #[serde(default = "default_transfer_time")]
    pub default_transfer_time: u32,

    /// Seconds to change between two train/RER/metro vehicles at one stop.
    /// IDFM merges every platform of a station into a single stop with no
    /// transfer time of its own, so without this a change at Gare du Nord or
    /// Châtelet would cost only `default_transfer_time`. Default: 300.
    #[serde(default = "default_rail_change_time")]
    pub rail_change_time: u32,

    /// Maximum journey duration in seconds.
    #[serde(default = "default_max_duration")]
    pub max_duration: u32,

    /// Seconds after midnight at which the service day starts. A query before
    /// it is answered on the previous day's services, shifted by 24 h, since
    /// GTFS files night runs under the day they started (25:30:00). Default:
    /// 14400 (04:00).
    #[serde(default = "default_service_day_start")]
    pub service_day_start: u32,

    /// Maximum walking distance to reach the nearest stop (meters).
    /// Coordinates beyond this radius will be rejected.
    /// Default: 1500 m (~20 min at 5 km/h).
    #[serde(default = "default_max_nearest_stop_distance")]
    pub max_nearest_stop_distance: u32,

    /// Radius (meters) of a second search, run only when an address origin or
    /// destination found no journey within `max_nearest_stop_distance` — the
    /// rural case, where the nearest useful stop is a long walk away. Equal to
    /// or below `max_nearest_stop_distance` disables it. Default: 2500.
    #[serde(default = "default_fallback_stop_distance")]
    pub fallback_stop_distance: u32,

    /// How many of the nearest candidate stops get their walk from an address
    /// timed by Valhalla before routing. The farther ones keep the straight-line
    /// estimate, scaled by the detour Valhalla measured on the near ones. Bounds
    /// the matrix request, whose cost grows with the stop count. Default: 30.
    #[serde(default = "default_walk_matrix_stops")]
    pub walk_matrix_stops: usize,

    /// Line-level diversity for alternatives. When `true`, the iterative search
    /// excludes the whole head line (all patterns of its route) after each
    /// journey, so successive alternatives depart on different lines instead of
    /// re-using the fastest corridor. Default: `false` (pattern-level diversity).
    #[serde(default = "default_diverse_lines")]
    pub diverse_lines: bool,

    /// Prefer rail over bus. When `true`, the search first finds journeys using
    /// only rail/metro/tram/train (buses forbidden); buses then only fill the
    /// remaining alternative slots. Default: `false`.
    #[serde(default = "default_prefer_rail")]
    pub prefer_rail: bool,

    /// `prefer_rail` only applies when a rail/metro/tram stop lies within this
    /// many seconds of walk of both the origin and the destination. Otherwise
    /// forbidding buses just trades a feeder bus for a long walk. Default: 600.
    #[serde(default = "default_prefer_rail_max_walk")]
    pub prefer_rail_max_walk: u32,

    /// Include turn-by-turn maneuvers (Valhalla) in walk/bike/car and transfer
    /// sections. Server-controlled only — not overridable per request. Default: `false`.
    #[serde(default = "default_maneuvers")]
    pub maneuvers: bool,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            max_journeys: default_max_journeys(),
            max_transfers: default_max_transfers(),
            default_transfer_time: default_transfer_time(),
            rail_change_time: default_rail_change_time(),
            max_duration: default_max_duration(),
            service_day_start: default_service_day_start(),
            max_nearest_stop_distance: default_max_nearest_stop_distance(),
            fallback_stop_distance: default_fallback_stop_distance(),
            walk_matrix_stops: default_walk_matrix_stops(),
            diverse_lines: default_diverse_lines(),
            prefer_rail: default_prefer_rail(),
            prefer_rail_max_walk: default_prefer_rail_max_walk(),
            maneuvers: default_maneuvers(),
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
fn default_diverse_lines() -> bool {
    false
}
fn default_prefer_rail() -> bool {
    false
}
fn default_service_day_start() -> u32 {
    4 * 3600
}
fn default_fallback_stop_distance() -> u32 {
    2500
}
fn default_rail_change_time() -> u32 {
    300
}
fn default_walk_matrix_stops() -> usize {
    30
}
fn default_prefer_rail_max_walk() -> u32 {
    600
}
fn default_maneuvers() -> bool {
    false
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

    /// Budget (seconds) for each pedestrian call made while answering a public
    /// transport search: walk matrices and first/last-mile legs. Kept short so
    /// a slow Valhalla degrades to estimates and missing shapes rather than a
    /// stalled search. Bike and car directions use the HTTP client's own
    /// timeout. Default: 5.
    #[serde(default = "default_valhalla_pedestrian_timeout_secs")]
    pub pedestrian_timeout_secs: u64,
}

impl Default for ValhallaConfig {
    fn default() -> Self {
        Self {
            host: default_valhalla_host(),
            port: default_valhalla_port(),
            pedestrian_timeout_secs: default_valhalla_pedestrian_timeout_secs(),
        }
    }
}

impl ValhallaConfig {
    /// Base URL of the Valhalla HTTP API, without a trailing slash.
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// [`Self::pedestrian_timeout_secs`] as a [`std::time::Duration`].
    pub fn pedestrian_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.pedestrian_timeout_secs)
    }
}

fn default_valhalla_host() -> String {
    "localhost".to_string()
}
fn default_valhalla_port() -> u16 {
    8002
}
fn default_valhalla_pedestrian_timeout_secs() -> u64 {
    5
}

// ---------------------------------------------------------------------------
// Pedestrian
// ---------------------------------------------------------------------------

/// Valhalla pedestrian costing for an ordinary walker. The wheelchair profile
/// ([`WheelchairConfig`]) replaces it when the request asks for one.
#[derive(Debug, Deserialize)]
pub struct PedestrianConfig {
    /// Walking speed in km/h when the request gives none. Used both for the
    /// straight-line estimates that pick candidate stops and for Valhalla, so
    /// the two agree. Default: 5.0.
    #[serde(default = "default_pedestrian_walking_speed")]
    pub walking_speed: f64,

    /// Valhalla penalty (seconds) per flight of stairs: a traveller may carry
    /// luggage or a pushchair. Station transfers ignore it. Default: 30.
    #[serde(default = "default_pedestrian_step_penalty")]
    pub step_penalty: f64,

    /// Valhalla penalty (seconds) for taking an elevator — mostly the wait.
    /// Station transfers ignore it. Default: 60.
    #[serde(default = "default_pedestrian_elevator_penalty")]
    pub elevator_penalty: f64,
}

impl Default for PedestrianConfig {
    fn default() -> Self {
        Self {
            walking_speed: default_pedestrian_walking_speed(),
            step_penalty: default_pedestrian_step_penalty(),
            elevator_penalty: default_pedestrian_elevator_penalty(),
        }
    }
}

fn default_pedestrian_walking_speed() -> f64 {
    5.0
}
fn default_pedestrian_step_penalty() -> f64 {
    30.0
}
fn default_pedestrian_elevator_penalty() -> f64 {
    60.0
}

// ---------------------------------------------------------------------------
// Map
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
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
    /// Placeholders: `{s}` (subdomain), `{z}`, `{x}`, `{y}`, `{r}` (retina),
    /// `{key}` ([`Self::tile_api_key`]).
    #[serde(default = "default_tile_url")]
    pub tile_url: String,

    /// API key of the tile provider, substituted for `{key}` in `tile_url`.
    /// Only the server's proxy uses it, so it never reaches the browser.
    #[serde(default)]
    pub tile_api_key: String,

    /// Browser cache duration for tiles, in seconds.
    #[serde(default = "default_tile_cache_duration")]
    pub tile_cache_duration: u32,
}

/// Hand-written so that `info!(?config)` at startup cannot print the tile
/// API key, nor a key written directly into `tile_url`.
impl std::fmt::Debug for MapConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapConfig")
            .field("center_lat", &self.center_lat)
            .field("center_lon", &self.center_lon)
            .field("zoom", &self.zoom)
            .field("bounds_sw_lat", &self.bounds_sw_lat)
            .field("bounds_sw_lon", &self.bounds_sw_lon)
            .field("bounds_ne_lat", &self.bounds_ne_lat)
            .field("bounds_ne_lon", &self.bounds_ne_lon)
            .field(
                "tile_url",
                &crate::shared::util::redact_query(&self.tile_url),
            )
            .field("tile_api_key_set", &!self.tile_api_key.is_empty())
            .field("tile_cache_duration", &self.tile_cache_duration)
            .finish()
    }
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
            tile_api_key: String::new(),
            tile_cache_duration: default_tile_cache_duration(),
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
fn default_tile_cache_duration() -> u32 {
    86400
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
// Wheelchair accessibility
// ---------------------------------------------------------------------------

/// Valhalla pedestrian costing overrides for wheelchair-accessible routing.
#[derive(Debug, Deserialize)]
pub struct WheelchairConfig {
    /// Penalty for stairs (very high = effectively avoid them).
    #[serde(default = "default_wheelchair_step_penalty")]
    pub step_penalty: f64,

    /// Maximum road grade in percent (6% is the standard wheelchair norm).
    #[serde(default = "default_wheelchair_max_grade")]
    pub max_grade: u32,

    /// Hill avoidance factor (0.0 = strongly avoid, 1.0 = no preference).
    #[serde(default = "default_wheelchair_use_hills")]
    pub use_hills: f64,

    /// Penalty for elevators (0 = prefer them).
    #[serde(default = "default_wheelchair_elevator_penalty")]
    pub elevator_penalty: f64,

    /// Wheelchair walking speed in km/h (typical: 3.5 km/h).
    #[serde(default = "default_wheelchair_walking_speed")]
    pub walking_speed: f64,
}

impl Default for WheelchairConfig {
    fn default() -> Self {
        Self {
            step_penalty: default_wheelchair_step_penalty(),
            max_grade: default_wheelchair_max_grade(),
            use_hills: default_wheelchair_use_hills(),
            elevator_penalty: default_wheelchair_elevator_penalty(),
            walking_speed: default_wheelchair_walking_speed(),
        }
    }
}

fn default_wheelchair_step_penalty() -> f64 {
    999999.0
}
fn default_wheelchair_max_grade() -> u32 {
    6
}
fn default_wheelchair_use_hills() -> f64 {
    0.0
}
fn default_wheelchair_elevator_penalty() -> f64 {
    0.0
}
fn default_wheelchair_walking_speed() -> f64 {
    3.5
}

// ---------------------------------------------------------------------------
// Traffic (Sytadin real-time road overlay)
// ---------------------------------------------------------------------------

/// Real-time road traffic overlay, sourced from the Sytadin (DiRIF) open
/// diffusion feed. Segment geometry is loaded once from local MIF/MID files
/// under `{data.dir}/sytadin`; dynamic states and events are polled from
/// `base_url` every `refresh_secs`.
#[derive(Debug, Deserialize)]
pub struct TrafficConfig {
    /// Enable the traffic overlay. When `false`, no geometry is loaded, no
    /// polling happens, and `GET /api/traffic` reports `enabled: false`.
    #[serde(default = "default_traffic_enabled")]
    pub enabled: bool,

    /// Base URL of the Sytadin diffusion feed (no trailing slash). Dynamic
    /// files are read from `{base_url}/xml/segments_dyn.xml` and
    /// `{base_url}/xml/evenements.xml`.
    #[serde(default = "default_traffic_base_url")]
    pub base_url: String,

    /// Interval, in seconds, between dynamic-data refreshes (feed updates ~1 min).
    #[serde(default = "default_traffic_refresh_secs")]
    pub refresh_secs: u64,
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            enabled: default_traffic_enabled(),
            base_url: default_traffic_base_url(),
            refresh_secs: default_traffic_refresh_secs(),
        }
    }
}

fn default_traffic_enabled() -> bool {
    false
}
fn default_traffic_base_url() -> String {
    "https://www.sytadin.fr/diffusion".to_string()
}
fn default_traffic_refresh_secs() -> u64 {
    60
}

// ---------------------------------------------------------------------------
// Real-time transit data
// ---------------------------------------------------------------------------

/// Real-time transit feeds (GTFS-Realtime today, SIRI once its connector
/// lands). Each feed is polled on its own interval; the resulting delays and
/// cancellations are merged into a single overlay consulted by the router.
#[derive(Debug, Deserialize, Default)]
pub struct RealtimeConfig {
    /// Enable real-time routing. When `false` nothing is polled and the
    /// router runs on the published schedule alone.
    #[serde(default)]
    pub enabled: bool,

    /// Feeds to poll, in precedence order: on conflict, the last one wins.
    #[serde(default)]
    pub feeds: Vec<FeedConfig>,
}

/// The wire format a feed speaks.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FeedKind {
    /// GTFS-Realtime protobuf (`FeedMessage`).
    GtfsRt,
}

impl std::fmt::Display for FeedKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GtfsRt => write!(f, "gtfs-rt"),
        }
    }
}

/// One pollable real-time endpoint.
#[derive(Deserialize, Clone)]
pub struct FeedConfig {
    /// Identifier shown in logs and on `GET /api/realtime/status`.
    pub name: String,

    /// Wire format. Configured as `type:` in YAML.
    #[serde(rename = "type")]
    pub kind: FeedKind,

    /// Endpoint to poll.
    pub url: String,

    /// Seconds between two polls.
    #[serde(default = "default_feed_refresh_secs")]
    pub refresh_secs: u64,

    /// Per-request timeout in seconds. Kept below `refresh_secs` so a stalled
    /// upstream cannot pile requests up.
    #[serde(default = "default_feed_timeout_secs")]
    pub timeout_secs: u64,

    /// Extra request headers, typically the provider's API key.
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

/// Hand-written so that `info!(?config)` at startup cannot print API keys:
/// header *names* are useful when debugging, their values never are.
impl std::fmt::Debug for FeedConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeedConfig")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("url", &crate::shared::util::redact_query(&self.url))
            .field("refresh_secs", &self.refresh_secs)
            .field("timeout_secs", &self.timeout_secs)
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .finish()
    }
}

fn default_feed_refresh_secs() -> u64 {
    30
}
fn default_feed_timeout_secs() -> u64 {
    10
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
                Ok(content) => match serde_yaml::from_str::<Self>(&content) {
                    Ok(mut config) => {
                        info!("Configuration loaded from {}", path.display());
                        config.apply_env_overrides(|key| std::env::var(key).ok());
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

        info!(
            "No config file found at {}, using defaults (template: config.yaml.sample)",
            path.display()
        );
        let mut config = Self::default();
        config.apply_env_overrides(|key| std::env::var(key).ok());
        config
    }

    /// Override deployment-specific settings from the environment.
    ///
    /// Only what differs between a host and a container: in Docker Compose
    /// Valhalla is another service (`valhalla`), not `localhost`, while the
    /// mounted `config.yaml` stays the one used on the host. The API key is
    /// a secret, so it can come from the environment instead of a file baked
    /// into an image; set but empty, it disables the protected endpoints.
    /// `lookup` stands in for `std::env::var` so tests need not mutate the
    /// process environment.
    fn apply_env_overrides(&mut self, lookup: impl Fn(&str) -> Option<String>) {
        if let Some(key) = lookup("GLOVE_API_KEY") {
            info!("server.api_key overridden by GLOVE_API_KEY");
            self.server.api_key = key;
        }
        if let Some(key) = lookup("GLOVE_TILE_API_KEY") {
            info!("map.tile_api_key overridden by GLOVE_TILE_API_KEY");
            self.map.tile_api_key = key;
        }
        if !self.map.tile_api_key.is_empty() && !self.map.tile_url.contains("{key}") {
            tracing::warn!("map.tile_api_key is set but map.tile_url has no {{key}} placeholder");
        }
        if let Some(host) = lookup("GLOVE_VALHALLA_HOST").filter(|h| !h.is_empty()) {
            info!("valhalla.host overridden by GLOVE_VALHALLA_HOST: {host}");
            self.valhalla.host = host;
        }
        match lookup("GLOVE_VALHALLA_PORT").map(|p| p.parse::<u16>()) {
            Some(Ok(port)) => {
                info!("valhalla.port overridden by GLOVE_VALHALLA_PORT: {port}");
                self.valhalla.port = port;
            }
            Some(Err(e)) => tracing::warn!("Ignoring invalid GLOVE_VALHALLA_PORT: {e}"),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_routing_setting_is_read_from_yaml() {
        // Unknown keys are ignored, so a misspelt key would silently keep its
        // default: set every field away from it and check each one lands.
        let yaml = "
            max_journeys: 7
            max_transfers: 2
            default_transfer_time: 90
            rail_change_time: 240
            max_duration: 3600
            service_day_start: 10800
            max_nearest_stop_distance: 1000
            fallback_stop_distance: 3000
            walk_matrix_stops: 12
            diverse_lines: true
            prefer_rail: true
            prefer_rail_max_walk: 420
            maneuvers: true
        ";
        let routing: super::RoutingConfig = serde_yaml::from_str(yaml).expect("valid routing");
        assert_eq!(routing.max_journeys, 7);
        assert_eq!(routing.max_transfers, 2);
        assert_eq!(routing.default_transfer_time, 90);
        assert_eq!(routing.rail_change_time, 240);
        assert_eq!(routing.max_duration, 3600);
        assert_eq!(routing.service_day_start, 10800);
        assert_eq!(routing.max_nearest_stop_distance, 1000);
        assert_eq!(routing.fallback_stop_distance, 3000);
        assert_eq!(routing.walk_matrix_stops, 12);
        assert!(routing.diverse_lines);
        assert!(routing.prefer_rail);
        assert_eq!(routing.prefer_rail_max_walk, 420);
        assert!(routing.maneuvers);
    }

    #[test]
    fn the_sample_config_loads() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.yaml.sample");
        let content = std::fs::read_to_string(path).expect("config.yaml.sample is tracked");
        let config: super::AppConfig = serde_yaml::from_str(&content).expect("sample parses");
        assert_eq!(config.routing.rail_change_time, 300);
        assert_eq!(config.routing.walk_matrix_stops, 30);
        assert_eq!(config.routing.service_day_start, 4 * 3600);
        assert_eq!(config.valhalla.pedestrian_timeout_secs, 5);
        assert_eq!(config.pedestrian.walking_speed, 5.0);
        assert_eq!(config.pedestrian.step_penalty, 30.0);
        assert_eq!(config.pedestrian.elevator_penalty, 60.0);
    }

    #[test]
    fn env_overrides_replace_valhalla_host_and_port() {
        let mut cfg = AppConfig::default();
        cfg.apply_env_overrides(|key| match key {
            "GLOVE_VALHALLA_HOST" => Some("valhalla".into()),
            "GLOVE_VALHALLA_PORT" => Some("8003".into()),
            _ => None,
        });
        assert_eq!(cfg.valhalla.host, "valhalla");
        assert_eq!(cfg.valhalla.port, 8003);
    }

    #[test]
    fn env_override_replaces_api_key_even_when_empty() {
        let mut cfg = AppConfig::default();
        cfg.server.api_key = "from-file".into();
        cfg.apply_env_overrides(|key| (key == "GLOVE_API_KEY").then(|| "from-env".into()));
        assert_eq!(cfg.server.api_key, "from-env");

        cfg.apply_env_overrides(|key| (key == "GLOVE_API_KEY").then(String::new));
        assert!(cfg.server.api_key.is_empty());
    }

    #[test]
    fn env_override_replaces_tile_api_key() {
        let mut cfg = AppConfig::default();
        cfg.apply_env_overrides(|key| (key == "GLOVE_TILE_API_KEY").then(|| "carto".into()));
        assert_eq!(cfg.map.tile_api_key, "carto");
    }

    #[test]
    fn map_config_debug_hides_tile_api_key() {
        let map = MapConfig {
            tile_url: "https://tiles.example/{z}/{x}/{y}.png?key=inline-secret".into(),
            tile_api_key: "config-secret".into(),
            ..MapConfig::default()
        };
        let printed = format!("{map:?}");
        assert!(!printed.contains("config-secret"));
        assert!(!printed.contains("inline-secret"));
        assert!(printed.contains("tile_api_key_set: true"));
    }

    #[test]
    fn env_overrides_ignore_empty_host_and_invalid_port() {
        let mut cfg = AppConfig::default();
        cfg.apply_env_overrides(|key| match key {
            "GLOVE_VALHALLA_HOST" => Some(String::new()),
            "GLOVE_VALHALLA_PORT" => Some("not-a-port".into()),
            _ => None,
        });
        assert_eq!(cfg.valhalla.host, "localhost");
        assert_eq!(cfg.valhalla.port, 8002);
    }

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
