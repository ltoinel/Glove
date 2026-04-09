//! Shared utilities.

use sha2::{Digest, Sha256};
use std::path::Path;

/// Compute a SHA-256 fingerprint of a directory based on file sizes.
///
/// Hashes each listed file's name and size. Files that don't exist are skipped.
/// Returns a hex-encoded hash string.
pub fn dir_fingerprint(dir: &Path, files: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for name in files {
        let path = dir.join(name);
        if let Ok(meta) = std::fs::metadata(&path) {
            hasher.update(name.as_bytes());
            hasher.update(meta.len().to_le_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

/// Compute a SHA-256 fingerprint by scanning a directory for matching files.
///
/// Finds files matching `prefix` and `suffix`, sorts them by name, and hashes
/// each file's name and size.
pub fn dir_fingerprint_glob(dir: &Path, prefix: &str, suffix: &str) -> String {
    let mut hasher = Sha256::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with(prefix) && n.ends_with(suffix))
            })
            .collect();
        files.sort_by_key(|e| e.file_name());
        for f in &files {
            if let Ok(meta) = f.metadata() {
                hasher.update(f.file_name().to_string_lossy().as_bytes());
                hasher.update(meta.len().to_le_bytes());
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

/// Parse a `"lon;lat"` string into `(lon, lat)`.
/// Parse a `"lon;lat"` string into `(lon, lat)`.
pub fn parse_coord(s: &str) -> Option<(f64, f64)> {
    let (lon_str, lat_str) = s.split_once(';')?;
    Some((lon_str.parse().ok()?, lat_str.parse().ok()?))
}

/// Parse and validate `from` and `to` coordinates for Valhalla endpoints.
///
/// Returns `Ok((from_lon, from_lat, to_lon, to_lat))` or an HTTP 400 error.
pub fn parse_from_to(
    from: &str,
    to: &str,
) -> Result<(f64, f64, f64, f64), actix_web::HttpResponse> {
    let (from_lon, from_lat) = parse_coord(from).ok_or_else(|| {
        actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "error": { "id": "bad_request", "message": "'from' must be in 'lon;lat' format" }
        }))
    })?;
    let (to_lon, to_lat) = parse_coord(to).ok_or_else(|| {
        actix_web::HttpResponse::BadRequest().json(serde_json::json!({
            "error": { "id": "bad_request", "message": "'to' must be in 'lon;lat' format" }
        }))
    })?;
    Ok((from_lon, from_lat, to_lon, to_lat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_coord_valid() {
        let (lon, lat) = parse_coord("2.347;48.858").unwrap();
        assert!((lon - 2.347).abs() < 1e-6);
        assert!((lat - 48.858).abs() < 1e-6);
    }

    #[test]
    fn parse_coord_invalid() {
        assert!(parse_coord("invalid").is_none());
        assert!(parse_coord("2.347").is_none());
        assert!(parse_coord("abc;def").is_none());
    }
}
