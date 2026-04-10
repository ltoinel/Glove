//! Map tile proxy with local disk cache.
//!
//! Proxies tile requests to a remote tile server (CARTO Voyager by default)
//! and caches the PNG files on disk under `data/tiles/{z}/{x}/{y}.png`.
//! Subsequent requests for the same tile are served directly from disk.

use actix_web::{HttpResponse, get, web};
use std::path::PathBuf;

use crate::config::AppConfig;

/// Subdomains for load balancing across tile servers.
const SUBDOMAINS: &[&str] = &["a", "b", "c", "d"];

/// Serve a map tile, from cache or upstream.
#[get("/api/tiles/{z}/{x}/{y}.png")]
pub async fn get_tile(
    path: web::Path<(u32, u32, u32)>,
    config: web::Data<AppConfig>,
) -> HttpResponse {
    let (z, x, y) = path.into_inner();

    // Validate zoom level and tile coordinates
    if z > 20 || x >= (1 << z) || y >= (1 << z) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": { "id": "bad_request", "message": "Invalid tile coordinates" }
        }));
    }

    let tiles_dir = config.data.tiles_dir();
    let tile_path = PathBuf::from(&tiles_dir)
        .join(z.to_string())
        .join(x.to_string())
        .join(format!("{y}.png"));

    // Serve from cache if available
    if tile_path.exists() {
        match std::fs::read(&tile_path) {
            Ok(data) => {
                return HttpResponse::Ok()
                    .content_type("image/png")
                    .append_header(("Cache-Control", format!("public, max-age={}", config.map.tile_cache_duration)))
                    .body(data);
            }
            Err(e) => {
                tracing::warn!("Failed to read cached tile {}: {e}", tile_path.display());
            }
        }
    }

    // Fetch from upstream
    let subdomain = SUBDOMAINS[(x as usize + y as usize) % SUBDOMAINS.len()];
    let url = config
        .map
        .tile_url
        .replace("{s}", subdomain)
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string())
        .replace("{r}", "");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to create HTTP client: {e}");
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": { "id": "tile_error", "message": "Internal error" }
            }));
        }
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to fetch tile from upstream: {e}");
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": { "id": "tile_error", "message": format!("Upstream tile server unreachable: {e}") }
            }));
        }
    };

    if !resp.status().is_success() {
        return HttpResponse::BadGateway().json(serde_json::json!({
            "error": { "id": "tile_error", "message": format!("Upstream returned {}", resp.status()) }
        }));
    }

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return HttpResponse::BadGateway().json(serde_json::json!({
                "error": { "id": "tile_error", "message": format!("Failed to read upstream response: {e}") }
            }));
        }
    };

    // Cache to disk (best-effort, don't fail the request if caching fails)
    if let Some(parent) = tile_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::debug!("Failed to create tile cache directory: {e}");
        } else if let Err(e) = std::fs::write(&tile_path, &bytes) {
            tracing::debug!("Failed to cache tile: {e}");
        }
    }

    HttpResponse::Ok()
        .content_type("image/png")
        .append_header(("Cache-Control", format!("public, max-age={}", config.map.tile_cache_duration)))
        .body(bytes)
}
