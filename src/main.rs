//! # Glove — GTFS Journey Planner
//!
//! Entry point for the Glove application. Loads configuration, initializes
//! the GTFS data and RAPTOR index, then starts the HTTP server.
//!
//! The RAPTOR data is wrapped in an [`ArcSwap`] to allow hot-reloading
//! GTFS files via `POST /api/reload` without restarting the server.

mod api;
mod config;
mod gtfs;
mod raptor;

use actix_web::{web, App, HttpServer};
use arc_swap::ArcSwap;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load YAML configuration (falls back to defaults if file is missing)
    let config = config::AppConfig::load(Path::new("config.yaml"));

    // Initialize structured logging with level from config (overridable via RUST_LOG)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level)),
        )
        .init();

    info!(?config);

    // Load raw GTFS CSV files into memory
    let gtfs = match gtfs::GtfsData::load(Path::new(&config.data_dir)) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to load GTFS data: {e}");
            std::process::exit(1);
        }
    };

    // Build the RAPTOR index from GTFS data (patterns, transfers, search index)
    let raptor_data = Arc::new(raptor::RaptorData::build(gtfs, config.default_transfer_time));
    info!(
        "{} patterns, {} stops",
        raptor_data.patterns.len(),
        raptor_data.stops.len()
    );

    // Wrap in ArcSwap for lock-free hot-reload support
    let shared_data = web::Data::new(ArcSwap::from(raptor_data));

    info!("Starting server on http://{}:{}", config.bind, config.port);

    let bind = config.bind.clone();
    let port = config.port;
    let workers = config.workers;
    let config = web::Data::new(config);

    let mut server = HttpServer::new(move || {
        App::new()
            .app_data(shared_data.clone())
            .app_data(config.clone())
            .service(api::get_places)
            .service(api::get_status)
            .service(api::get_journeys)
            .service(api::post_reload)
    });

    // Use configured worker count, or let actix auto-detect (one per logical CPU)
    if workers > 0 {
        server = server.workers(workers);
    }

    server.bind((bind.as_str(), port))?.run().await
}
