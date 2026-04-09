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
    // Load YAML configuration (falls back to defaults if file is missing)
    let config = config::AppConfig::load(Path::new("config.yaml"));

    // Initialize structured logging with level from config (overridable via RUST_LOG)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.server.log_level)),
        )
        .init();

    info!(?config);

    // Try loading RAPTOR index from cache, or build from GTFS
    let data_dir = config.data.gtfs_dir();
    let cache_dir = config.data.raptor_dir();
    let data_dir = Path::new(&data_dir);
    let cache_dir = Path::new(&cache_dir);
    let fingerprint = gtfs::gtfs_fingerprint(data_dir);

    let raptor_data = if let Some(cached) = raptor::RaptorData::load_cached(cache_dir, &fingerprint)
    {
        Arc::new(cached)
    } else {
        let gtfs = match gtfs::GtfsData::load(data_dir) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to load GTFS data: {e}");
                std::process::exit(1);
            }
        };
        let data = raptor::RaptorData::build(gtfs, config.routing.default_transfer_time);
        if let Err(e) = data.save(cache_dir, &fingerprint) {
            tracing::warn!("Failed to save RAPTOR cache: {e}");
        }
        Arc::new(data)
    };
    info!(
        "{} patterns, {} stops",
        raptor_data.patterns.len(),
        raptor_data.stops.len()
    );

    // Wrap in ArcSwap for lock-free hot-reload support
    let shared_data = web::Data::new(ArcSwap::from(raptor_data));

    // Load BAN address data (from cache or CSV)
    let ban_dir = config.data.ban_dir();
    let ban_dir = Path::new(&ban_dir);
    let ban_fingerprint = ban::BanData::fingerprint(ban_dir);
    let ban_data = if let Some(cached) = ban::BanData::load_cached(ban_dir, &ban_fingerprint) {
        Arc::new(cached)
    } else {
        let data = ban::BanData::load(ban_dir);
        if let Err(e) = data.save(ban_dir, &ban_fingerprint) {
            tracing::warn!("Failed to save BAN cache: {e}");
        }
        Arc::new(data)
    };
    let shared_ban = web::Data::new(ban_data);

    // Initialize metrics start time
    api::metrics::init_start_time();

    info!(
        "Starting server on http://{}:{}",
        config.server.bind, config.server.port
    );

    // Warn if no API key is set — admin endpoints will be disabled
    if config.server.api_key.is_empty() {
        warn!("No server.api_key configured — POST /api/gtfs/reload is DISABLED");
    }

    let bind = config.server.bind.clone();
    let port = config.server.port;
    let workers = config.server.workers;
    let shutdown_timeout = config.server.shutdown_timeout;
    let cors_origins = config.server.cors_origins.clone();
    let rate_limit = config.server.rate_limit;
    let config = web::Data::new(config);

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

    let openapi_json = match ApiDoc::openapi().to_json() {
        Ok(json) => web::Data::new(json),
        Err(e) => {
            eprintln!("Failed to generate OpenAPI spec: {e}");
            std::process::exit(1);
        }
    };

    // Build rate-limiter governor
    // When rate_limit is 0 (disabled), use a very high burst to effectively disable limiting
    let effective_burst = if rate_limit > 0 { rate_limit } else { u32::MAX };
    let governor_conf = GovernorConfigBuilder::default()
        .seconds_per_request(1)
        .burst_size(effective_burst)
        .finish()
        .expect("valid governor config");

    let mut server = HttpServer::new(move || {
        // --- CORS ---
        let cors = if cors_origins.iter().any(|o| o == "*") {
            Cors::permissive()
        } else if cors_origins.is_empty() {
            Cors::default()
        } else {
            let mut c = Cors::default()
                .allowed_methods(vec!["GET", "POST", "OPTIONS"])
                .allowed_headers(vec!["Content-Type", "Authorization", "X-Api-Key"])
                .max_age(3600);
            for origin in &cors_origins {
                c = c.allowed_origin(origin);
            }
            c
        };

        App::new()
            .wrap(cors)
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

    // Use configured worker count, or let actix auto-detect (one per logical CPU)
    if workers > 0 {
        server = server.workers(workers);
    }

    // Graceful shutdown: allow in-flight requests to complete
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
