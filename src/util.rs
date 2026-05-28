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

    #[test]
    fn parse_from_to_both_valid() {
        let (flon, flat, tlon, tlat) = parse_from_to("2.3;48.8", "2.4;48.9").unwrap();
        assert!((flon - 2.3).abs() < 1e-6);
        assert!((flat - 48.8).abs() < 1e-6);
        assert!((tlon - 2.4).abs() < 1e-6);
        assert!((tlat - 48.9).abs() < 1e-6);
    }

    #[test]
    fn parse_from_to_bad_from_returns_400() {
        let err = parse_from_to("bad", "2.4;48.9").unwrap_err();
        assert_eq!(err.status(), 400);
    }

    #[test]
    fn parse_from_to_bad_to_returns_400() {
        let err = parse_from_to("2.3;48.8", "bad").unwrap_err();
        assert_eq!(err.status(), 400);
    }

    #[test]
    fn dir_fingerprint_includes_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let path_a = dir.path().join("a.txt");
        std::fs::write(&path_a, b"hello").unwrap();
        let fp1 = dir_fingerprint(dir.path(), &["a.txt", "missing.txt"]);
        // changing file size must change the fingerprint
        std::fs::write(&path_a, b"hello world").unwrap();
        let fp2 = dir_fingerprint(dir.path(), &["a.txt", "missing.txt"]);
        assert_ne!(fp1, fp2);
        // hex-encoded SHA-256 = 64 chars
        assert_eq!(fp1.len(), 64);
    }

    #[test]
    fn dir_fingerprint_glob_picks_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("data-01.csv"), b"a").unwrap();
        std::fs::write(dir.path().join("data-02.csv"), b"b").unwrap();
        std::fs::write(dir.path().join("readme.md"), b"ignore me").unwrap();
        let fp = dir_fingerprint_glob(dir.path(), "data-", ".csv");
        assert_eq!(fp.len(), 64);

        // Removing a matching file changes the fingerprint
        std::fs::remove_file(dir.path().join("data-02.csv")).unwrap();
        let fp2 = dir_fingerprint_glob(dir.path(), "data-", ".csv");
        assert_ne!(fp, fp2);
    }

    #[test]
    fn dir_fingerprint_glob_returns_stable_value_for_empty() {
        let dir = tempfile::tempdir().unwrap();
        let fp = dir_fingerprint_glob(dir.path(), "data-", ".csv");
        assert_eq!(fp.len(), 64);
    }
}
