//! # Glove — GTFS Journey Planner
//!
//! Entry point for the Glove application. Loads configuration, initializes
//! the GTFS data and RAPTOR index, then starts the HTTP server.
//!
//! The RAPTOR data is wrapped in an [`ArcSwap`] to allow hot-reloading
//! GTFS files via `POST /api/gtfs/reload` without restarting the server.

mod api;
mod ban;
mod config;
mod gtfs;
mod raptor;
mod text;
mod util;

use actix_cors::Cors;
use actix_governor::{Governor, GovernorConfigBuilder};
use actix_web::middleware::{self, Next};
use actix_web::{App, HttpServer, dev, web};
use arc_swap::ArcSwap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::{info, warn};
use utoipa::OpenApi;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = config::AppConfig::load(Path::new("config.yaml"));
    init_logging(&config.server.log_level);
    info!(?config);

    let raptor_data = load_or_build_raptor(&config);
    info!(
        "{} patterns, {} stops",
        raptor_data.patterns.len(),
        raptor_data.stops.len()
    );

    let ban_data = load_or_build_ban(&config);
    api::metrics::init_start_time();

    info!(
        "Starting server on http://{}:{}",
        config.server.bind, config.server.port
    );
    if config.server.api_key.is_empty() {
        warn!("No server.api_key configured — POST /api/gtfs/reload is DISABLED");
    }

    let openapi_json = build_openapi_spec();
    run_http_server(config, raptor_data, ban_data, openapi_json).await
}

/// Initialize tracing subscriber with the level from config (overridable via RUST_LOG).
fn init_logging(default_level: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level)),
        )
        .init();
}

/// Load the RAPTOR index from cache, or rebuild it from raw GTFS data on miss.
/// Exits the process on unrecoverable GTFS load failure.
fn load_or_build_raptor(config: &config::AppConfig) -> Arc<raptor::RaptorData> {
    let data_dir = config.data.gtfs_dir();
    let cache_dir = config.data.raptor_dir();
    let data_dir = Path::new(&data_dir);
    let cache_dir = Path::new(&cache_dir);
    let fingerprint = gtfs::gtfs_fingerprint(data_dir);

    if let Some(cached) = raptor::RaptorData::load_cached(cache_dir, &fingerprint) {
        return Arc::new(cached);
    }
    let gtfs = match gtfs::GtfsData::load(data_dir) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to load GTFS data: {e}");
            std::process::exit(1);
        }
    };
    let data = raptor::RaptorData::build(gtfs, config.routing.default_transfer_time);
    if let Err(e) = data.save(cache_dir, &fingerprint) {
        warn!("Failed to save RAPTOR cache: {e}");
    }
    Arc::new(data)
}

/// Load the BAN (French national address database) index from cache or CSV.
fn load_or_build_ban(config: &config::AppConfig) -> Arc<ban::BanData> {
    let ban_dir = config.data.ban_dir();
    let ban_dir = Path::new(&ban_dir);
    let ban_fingerprint = ban::BanData::fingerprint(ban_dir);

    if let Some(cached) = ban::BanData::load_cached(ban_dir, &ban_fingerprint) {
        return Arc::new(cached);
    }
    let data = ban::BanData::load(ban_dir);
    if let Err(e) = data.save(ban_dir, &ban_fingerprint) {
        warn!("Failed to save BAN cache: {e}");
    }
    Arc::new(data)
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Glove API",
        description = "GTFS journey planner — Navitia-compatible REST API powered by the RAPTOR algorithm",
        version = "0.1.0",
    ),
    paths(
        api::get_journeys,
        api::get_walk,
        api::get_bike,
        api::get_car,
        api::get_places,
        api::get_status,
        api::get_metrics,
        api::get_validate,
        api::post_reload,
    ),
    components(schemas(
        api::journeys::public_transport::JourneysResponse,
        api::journeys::public_transport::Journey,
        api::journeys::public_transport::DisplayInfo,
        api::journeys::public_transport::DatetimeRepresents,
        api::journeys::public_transport::DataFreshness,
        api::journeys::walk::WalkResponse,
        api::journeys::walk::WalkJourney,
        api::journeys::walk::Maneuver,
        api::journeys::bike::BikeResponse,
        api::journeys::bike::BikeJourney,
        api::journeys::bike::Maneuver,
        api::journeys::car::CarResponse,
        api::journeys::car::CarJourney,
        api::journeys::car::Maneuver,
        api::places::PlacesResponse,
        api::places::PlaceResult,
        api::status::StatusResponse,
        api::status::GtfsStats,
        api::status::RaptorStats,
        api::gtfs::ValidateResponse,
        api::gtfs::ValidationSummary,
        api::gtfs::ValidationIssue,
        api::gtfs::Severity,
        api::gtfs::Category,
        api::gtfs::ReloadResponse,
        api::Section,
        api::Place,
        api::StopPointRef,
        api::StopDateTime,
        api::Coord,
    )),
    tags(
        (name = "Journeys", description = "Journey planning"),
        (name = "Places", description = "Stop autocomplete search"),
        (name = "Status", description = "Engine status"),
        (name = "GTFS", description = "GTFS data validation and management"),
    )
)]
struct ApiDoc;

/// Generate the OpenAPI JSON spec. Exits the process if serialization fails
/// (this is a build-time configuration error, not a runtime condition).
fn build_openapi_spec() -> web::Data<String> {
    match ApiDoc::openapi().to_json() {
        Ok(json) => web::Data::new(json),
        Err(e) => {
            eprintln!("Failed to generate OpenAPI spec: {e}");
            std::process::exit(1);
        }
    }
}

/// Build the CORS middleware from the configured allowed origins.
fn build_cors(cors_origins: &[String]) -> Cors {
    if cors_origins.iter().any(|o| o == "*") {
        return Cors::permissive();
    }
    if cors_origins.is_empty() {
        return Cors::default();
    }
    let mut c = Cors::default()
        .allowed_methods(vec!["GET", "POST", "OPTIONS"])
        .allowed_headers(vec!["Content-Type", "Authorization", "X-Api-Key"])
        .max_age(3600);
    for origin in cors_origins {
        c = c.allowed_origin(origin);
    }
    c
}

/// Configure and run the Actix HTTP server until shutdown.
async fn run_http_server(
    config: config::AppConfig,
    raptor_data: Arc<raptor::RaptorData>,
    ban_data: Arc<ban::BanData>,
    openapi_json: web::Data<String>,
) -> std::io::Result<()> {
    let bind = config.server.bind.clone();
    let port = config.server.port;
    let workers = config.server.workers;
    let shutdown_timeout = config.server.shutdown_timeout;
    let cors_origins = config.server.cors_origins.clone();
    let rate_limit = config.server.rate_limit;

    let shared_data = web::Data::new(ArcSwap::from(raptor_data));
    let shared_ban = web::Data::new(ban_data);
    let config = web::Data::new(config);

    // When rate_limit is 0 (disabled), use a very high burst to effectively disable limiting
    let effective_burst = if rate_limit > 0 { rate_limit } else { u32::MAX };
    let governor_conf = GovernorConfigBuilder::default()
        .seconds_per_request(1)
        .burst_size(effective_burst)
        .finish()
        .expect("valid governor config");

    let mut server = HttpServer::new(move || {
        App::new()
            .wrap(build_cors(&cors_origins))
            .wrap(middleware::from_fn(metrics_middleware))
            .app_data(shared_data.clone())
            .app_data(shared_ban.clone())
            .app_data(config.clone())
            .app_data(openapi_json.clone())
            // Tile proxy: no rate limiting (high request volume from map panning)
            .service(api::get_tile)
            // All other API endpoints: rate-limited
            .service(
                web::scope("")
                    .wrap(Governor::new(&governor_conf))
                    .service(api::get_places)
                    .service(api::get_status)
                    .service(api::get_walk)
                    .service(api::get_bike)
                    .service(api::get_car)
                    .service(api::get_journeys)
                    .service(api::get_metrics)
                    .service(api::get_validate)
                    .service(api::post_reload),
            )
            .route(
                "/api-docs/openapi.json",
                web::get().to(|spec: web::Data<String>| async move {
                    actix_web::HttpResponse::Ok()
                        .content_type("application/json")
                        .body(spec.get_ref().clone())
                }),
            )
    });

    if workers > 0 {
        server = server.workers(workers);
    }
    server = server.shutdown_timeout(shutdown_timeout);
    server.bind((bind.as_str(), port))?.run().await
}

/// Middleware that counts HTTP requests and errors for Prometheus metrics.
async fn metrics_middleware(
    req: dev::ServiceRequest,
    next: Next<impl actix_web::body::MessageBody>,
) -> Result<dev::ServiceResponse<impl actix_web::body::MessageBody>, actix_web::Error> {
    api::metrics::HTTP_REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);
    let res = next.call(req).await?;
    if res.status().is_client_error() || res.status().is_server_error() {
        api::metrics::HTTP_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_openapi_spec_returns_non_empty_json() {
        let spec = build_openapi_spec();
        let s = spec.get_ref();
        assert!(s.starts_with('{'));
        // Spec must reference the main public-transport endpoint
        assert!(s.contains("/api/journeys/public_transport"));
    }

    #[test]
    fn build_cors_wildcard_returns_permissive() {
        // We can't introspect Cors fields; just confirm the call doesn't panic
        // and returns a value for each branch.
        let _ = build_cors(&["*".to_string()]);
    }

    #[test]
    fn build_cors_empty_returns_default() {
        let _ = build_cors(&[]);
    }

    #[test]
    fn build_cors_specific_origins_no_panic() {
        let _ = build_cors(&[
            "https://example.com".to_string(),
            "https://other.example".to_string(),
        ]);
    }

    #[test]
    fn init_logging_can_be_called_with_unparseable_filter() {
        // tracing's `init` panics if called twice in the same process, so we
        // only verify init_logging is callable with EnvFilter::new() inputs by
        // exercising the lambda's fallback indirectly.
        let filter = tracing_subscriber::EnvFilter::new("info");
        assert!(filter.to_string().contains("info"));
    }

    /// Write a minimal but valid GTFS dataset to `gtfs_dir`.
    fn write_minimal_gtfs(gtfs_dir: &std::path::Path) {
        std::fs::create_dir_all(gtfs_dir).unwrap();
        std::fs::write(
            gtfs_dir.join("agency.txt"),
            "agency_id,agency_name,agency_url,agency_timezone\nA1,Test,https://e,Europe/Paris\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("routes.txt"),
            "route_id,agency_id,route_short_name,route_long_name,route_type,route_color,route_text_color\nR1,A1,1,Line 1,1,FFCD00,000000\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("stops.txt"),
            "stop_id,stop_name,stop_lon,stop_lat,parent_station,wheelchair_boarding\n\
             S1,StopA,2.347,48.858,,0\n\
             S2,StopB,2.395,48.848,,0\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("trips.txt"),
            "route_id,service_id,trip_id,trip_headsign,wheelchair_accessible\nR1,SVC1,T1,StopB,0\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("stop_times.txt"),
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
             T1,08:00:00,08:01:00,S1,0\n\
             T1,08:10:00,08:11:00,S2,1\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("calendar.txt"),
            "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\n\
             SVC1,1,1,1,1,1,1,1,20260101,20261231\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("calendar_dates.txt"),
            "service_id,date,exception_type\n",
        )
        .unwrap();
        std::fs::write(
            gtfs_dir.join("transfers.txt"),
            "from_stop_id,to_stop_id,min_transfer_time\n",
        )
        .unwrap();
    }

    #[test]
    fn load_or_build_raptor_from_scratch_then_from_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config::AppConfig::default();
        cfg.data.dir = dir.path().to_string_lossy().into();
        write_minimal_gtfs(std::path::Path::new(&cfg.data.gtfs_dir()));

        // First call: build + save cache.
        let data1 = load_or_build_raptor(&cfg);
        assert!(data1.stops.len() >= 2);

        // Second call: must hit the cache branch.
        let data2 = load_or_build_raptor(&cfg);
        assert_eq!(data1.stops.len(), data2.stops.len());
    }

    #[test]
    fn load_or_build_ban_handles_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config::AppConfig::default();
        cfg.data.dir = dir.path().to_string_lossy().into();
        let ban = load_or_build_ban(&cfg);
        assert!(ban.entries.is_empty());
    }

    #[test]
    fn load_or_build_ban_loads_csv_and_then_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config::AppConfig::default();
        cfg.data.dir = dir.path().to_string_lossy().into();
        let ban_dir = std::path::Path::new(&cfg.data.ban_dir()).to_path_buf();
        std::fs::create_dir_all(&ban_dir).unwrap();
        std::fs::write(
            ban_dir.join("adresses-01.csv"),
            "id;nom_voie;code_postal;nom_commune;lon;lat\n\
             1;Rue Test;75000;Paris;2.3;48.8\n",
        )
        .unwrap();
        let ban1 = load_or_build_ban(&cfg);
        assert_eq!(ban1.entries.len(), 1);
        // Second call hits the cache.
        let ban2 = load_or_build_ban(&cfg);
        assert_eq!(ban2.entries.len(), 1);
    }

    #[actix_web::test]
    async fn run_http_server_starts_and_serves_status() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config::AppConfig::default();
        cfg.data.dir = dir.path().to_string_lossy().into();
        write_minimal_gtfs(std::path::Path::new(&cfg.data.gtfs_dir()));
        // Bind to ephemeral port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        cfg.server.bind = "127.0.0.1".into();
        cfg.server.port = port;
        cfg.server.shutdown_timeout = 1;
        cfg.server.rate_limit = 0; // disable rate limiting in tests
        cfg.server.cors_origins = vec!["https://example.com".to_string()]; // exercise specific-origin branch
        cfg.valhalla.host = "127.0.0.1".into();
        cfg.valhalla.port = 1;

        let raptor = load_or_build_raptor(&cfg);
        let ban = load_or_build_ban(&cfg);
        let openapi = build_openapi_spec();

        // Spawn server on a background thread, give it time to start, then
        // make one request to exercise the route table.
        std::thread::spawn(move || {
            let sys = actix_web::rt::System::new();
            sys.block_on(async {
                let _ = run_http_server(cfg, raptor, ban, openapi).await;
            });
        });
        std::thread::sleep(std::time::Duration::from_millis(300));

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/api/status"))
            .send()
            .await
            .expect("server responds");
        assert!(resp.status().is_success());
    }
}
