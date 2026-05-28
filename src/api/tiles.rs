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

    if let Err(resp) = validate_tile_coords(z, x, y) {
        return resp;
    }

    let tile_path = match resolve_tile_path(&config, z, x, y) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    if let Some(resp) = serve_cached_tile(&tile_path, config.map.tile_cache_duration) {
        return resp;
    }

    let url = build_upstream_url(&config.map.tile_url, z, x, y);
    let bytes = match fetch_upstream_tile(&url).await {
        Ok(b) => b,
        Err(resp) => return resp,
    };

    cache_tile_to_disk(&tile_path, &bytes);

    HttpResponse::Ok()
        .content_type("image/png")
        .append_header((
            "Cache-Control",
            format!("public, max-age={}", config.map.tile_cache_duration),
        ))
        .body(bytes)
}

fn bad_request(message: &str) -> HttpResponse {
    HttpResponse::BadRequest().json(serde_json::json!({
        "error": { "id": "bad_request", "message": message }
    }))
}

fn tile_error(message: String) -> HttpResponse {
    HttpResponse::BadGateway().json(serde_json::json!({
        "error": { "id": "tile_error", "message": message }
    }))
}

fn validate_tile_coords(z: u32, x: u32, y: u32) -> Result<(), HttpResponse> {
    if z > 20 || x >= (1 << z) || y >= (1 << z) {
        return Err(bad_request("Invalid tile coordinates"));
    }
    Ok(())
}

/// Resolve `{tiles_dir}/{z}/{x}/{y}.png` and verify it stays inside the
/// configured tile cache directory (defense in depth — z/x/y are u32 already).
fn resolve_tile_path(config: &AppConfig, z: u32, x: u32, y: u32) -> Result<PathBuf, HttpResponse> {
    let tiles_dir = config.data.tiles_dir();
    let base = PathBuf::from(&tiles_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&tiles_dir));
    let tile_path = base
        .join(z.to_string())
        .join(x.to_string())
        .join(format!("{y}.png"));
    if !tile_path.starts_with(&base) {
        return Err(bad_request("Invalid tile path"));
    }
    Ok(tile_path)
}

/// Return a cached response if the tile is on disk and readable.
fn serve_cached_tile(tile_path: &std::path::Path, cache_duration: u32) -> Option<HttpResponse> {
    if !tile_path.exists() {
        return None;
    }
    match std::fs::read(tile_path) {
        Ok(data) => Some(
            HttpResponse::Ok()
                .content_type("image/png")
                .append_header(("Cache-Control", format!("public, max-age={cache_duration}")))
                .body(data),
        ),
        Err(e) => {
            tracing::warn!("Failed to read cached tile {}: {e}", tile_path.display());
            None
        }
    }
}

fn build_upstream_url(template: &str, z: u32, x: u32, y: u32) -> String {
    let subdomain = SUBDOMAINS[(x as usize + y as usize) % SUBDOMAINS.len()];
    template
        .replace("{s}", subdomain)
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string())
        .replace("{r}", "")
}

async fn fetch_upstream_tile(url: &str) -> Result<Vec<u8>, HttpResponse> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| {
            tracing::error!("Failed to create HTTP client: {e}");
            tile_error("Internal error".into())
        })?;

    let resp = client.get(url).send().await.map_err(|e| {
        tracing::warn!("Failed to fetch tile from upstream: {e}");
        tile_error(format!("Upstream tile server unreachable: {e}"))
    })?;

    if !resp.status().is_success() {
        return Err(tile_error(format!("Upstream returned {}", resp.status())));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| tile_error(format!("Failed to read upstream response: {e}")))
}

/// Persist the tile to disk. Best-effort: caching failures are logged but
/// never propagated, because the request itself has succeeded.
fn cache_tile_to_disk(tile_path: &std::path::Path, bytes: &[u8]) {
    let Some(parent) = tile_path.parent() else {
        return;
    };
    if let Err(e) = std::fs::create_dir_all(parent) {
        tracing::debug!("Failed to create tile cache directory: {e}");
        return;
    }
    if let Err(e) = std::fs::write(tile_path, bytes) {
        tracing::debug!("Failed to cache tile: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_tile_coords_accepts_valid_range() {
        assert!(validate_tile_coords(0, 0, 0).is_ok());
        assert!(validate_tile_coords(10, 0, 0).is_ok());
        assert!(validate_tile_coords(20, (1 << 20) - 1, (1 << 20) - 1).is_ok());
    }

    #[test]
    fn validate_tile_coords_rejects_excessive_zoom() {
        assert!(validate_tile_coords(21, 0, 0).is_err());
    }

    #[test]
    fn validate_tile_coords_rejects_out_of_range_x_y() {
        assert!(validate_tile_coords(5, 1 << 5, 0).is_err());
        assert!(validate_tile_coords(5, 0, 1 << 5).is_err());
    }

    #[test]
    fn build_upstream_url_substitutes_placeholders() {
        let url = build_upstream_url("https://{s}.tile.example/{z}/{x}/{y}{r}.png", 12, 3, 7);
        // subdomain = SUBDOMAINS[(3+7) % 4] = SUBDOMAINS[2] = "c"
        assert_eq!(url, "https://c.tile.example/12/3/7.png");
    }

    #[test]
    fn build_upstream_url_subdomain_cycles() {
        // (x+y) % 4 selects the subdomain
        let url0 = build_upstream_url("https://{s}.tile/{z}/{x}/{y}.png", 1, 0, 0);
        let url1 = build_upstream_url("https://{s}.tile/{z}/{x}/{y}.png", 1, 0, 1);
        assert!(url0.starts_with("https://a."));
        assert!(url1.starts_with("https://b."));
    }

    #[test]
    fn bad_request_returns_400() {
        let resp = bad_request("bad coords");
        assert_eq!(resp.status(), 400);
    }

    #[test]
    fn tile_error_returns_502() {
        let resp = tile_error("upstream down".into());
        assert_eq!(resp.status(), 502);
    }

    #[test]
    fn serve_cached_tile_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.png");
        assert!(serve_cached_tile(&missing, 60).is_none());
    }

    #[test]
    fn serve_cached_tile_returns_response_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tile.png");
        std::fs::write(&path, b"PNGDATA").unwrap();
        let resp = serve_cached_tile(&path, 60).expect("cached response");
        assert_eq!(resp.status(), 200);
    }

    #[test]
    fn cache_tile_to_disk_creates_parent_dirs_and_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("12").join("3").join("7.png");
        cache_tile_to_disk(&path, b"DATA");
        assert_eq!(std::fs::read(&path).unwrap(), b"DATA");
    }

    #[test]
    fn cache_tile_to_disk_is_silent_on_invalid_path() {
        // Writing to a path whose parent cannot be created (a file used as a dir)
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let path = blocker.join("nested.png");
        cache_tile_to_disk(&path, b"DATA"); // must not panic
    }

    fn make_config_with_tiles_dir(dir: &std::path::Path) -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.data.dir = dir.to_string_lossy().into_owned();
        cfg.map.tile_url = "http://127.0.0.1:1/{z}/{x}/{y}.png".into();
        cfg
    }

    #[actix_web::test]
    async fn get_tile_rejects_invalid_coords() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = make_config_with_tiles_dir(dir.path());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(cfg))
                .service(get_tile),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/tiles/25/0/0.png")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn get_tile_serves_cached_file() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = make_config_with_tiles_dir(dir.path());
        let tiles_dir = cfg.data.tiles_dir();
        let cached = std::path::Path::new(&tiles_dir).join("5/3/7.png");
        std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
        std::fs::write(&cached, b"PNG-CACHED").unwrap();

        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(cfg))
                .service(get_tile),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/tiles/5/3/7.png")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = actix_web::test::read_body(resp).await;
        assert_eq!(&body[..], b"PNG-CACHED");
    }

    fn spawn_mock_tile_server(body: Vec<u8>) -> String {
        use actix_web::{App, HttpResponse, HttpServer, get};
        use std::net::TcpListener;

        #[get("/{z}/{x}/{y}.png")]
        async fn tile_handler(data: actix_web::web::Data<Vec<u8>>) -> HttpResponse {
            HttpResponse::Ok()
                .content_type("image/png")
                .body(data.get_ref().clone())
        }

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();
        let url = format!("http://127.0.0.1:{port}/{{z}}/{{x}}/{{y}}.png");
        let body_clone = body.clone();
        std::thread::spawn(move || {
            let sys = actix_web::rt::System::new();
            sys.block_on(async {
                let data = actix_web::web::Data::new(body_clone);
                let server = HttpServer::new(move || {
                    App::new().app_data(data.clone()).service(tile_handler)
                })
                .listen(listener)
                .unwrap()
                .workers(1)
                .run();
                let _ = server.await;
            });
        });
        std::thread::sleep(std::time::Duration::from_millis(150));
        url
    }

    #[actix_web::test]
    async fn get_tile_fetches_and_caches_from_upstream() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = make_config_with_tiles_dir(dir.path());
        cfg.map.tile_url = spawn_mock_tile_server(b"PNG-FROM-UPSTREAM".to_vec());
        let tiles_dir = cfg.data.tiles_dir();
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(cfg))
                .service(get_tile),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/tiles/4/2/3.png")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = actix_web::test::read_body(resp).await;
        assert_eq!(&body[..], b"PNG-FROM-UPSTREAM");
        // The tile must have been cached to disk
        let cached = std::path::Path::new(&tiles_dir).join("4/2/3.png");
        assert!(cached.exists());
    }

    #[actix_web::test]
    async fn get_tile_upstream_unreachable_returns_502() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = make_config_with_tiles_dir(dir.path());
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(web::Data::new(cfg))
                .service(get_tile),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/api/tiles/3/2/4.png")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 502);
    }
}
