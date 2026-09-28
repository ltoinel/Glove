//! Shared utilities.

use sha2::{Digest, Sha256};
use std::collections::BinaryHeap;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tracing::debug;

/// Strip a URL's query string, replacing it with `?…` when one was present.
///
/// Real-time providers commonly pass the API key as a query parameter, so a
/// feed URL is a credential. Logs and the status endpoint show the endpoint
/// without it.
pub fn redact_query(url: &str) -> String {
    match url.split_once('?') {
        Some((base, _)) => format!("{base}?…"),
        None => url.to_string(),
    }
}

/// Resolve `dir` to its canonical absolute form, or keep it as given when it
/// does not exist yet.
///
/// Callers build file paths under the result and then pass them through
/// [`reject_parent_traversal`]; canonicalizing first is what lets a
/// legitimately relative data directory (`../data`) pass that check.
pub fn canonical_dir(dir: &Path) -> PathBuf {
    dir.canonicalize().unwrap_or_else(|e| {
        debug!("Cannot canonicalize {}: {e}", dir.display());
        dir.to_path_buf()
    })
}

/// Refuse a filesystem path that contains `..`.
///
/// A plain substring test rather than a component walk: stricter (it also
/// refuses `a..b`, which no path of ours contains) and the form CodeQL's
/// path-injection query recognizes as a sanitizer. The paths it guards are
/// built from config and request values, so anything climbing out of the
/// base directory is refused before it reaches the filesystem.
pub fn reject_parent_traversal(path: String) -> Option<PathBuf> {
    if path.contains("..") {
        return None;
    }
    Some(PathBuf::from(path))
}

/// Feed one file's identity into a fingerprint: name, size and modification time.
///
/// Size alone misses a same-size rewrite (a GTFS export whose times shifted but
/// whose byte count did not), so the mtime is hashed too, to the nanosecond.
/// A platform without usable mtimes only loses that extra signal: the error is
/// logged and name + size are still hashed.
fn hash_file_metadata(hasher: &mut Sha256, name: &str, meta: &Metadata) {
    hasher.update(name.as_bytes());
    hasher.update(meta.len().to_le_bytes());
    let mtime = meta
        .modified()
        .map_err(|e| e.to_string())
        .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(|e| e.to_string()));
    match mtime {
        Ok(since_epoch) => {
            hasher.update(since_epoch.as_secs().to_le_bytes());
            hasher.update(since_epoch.subsec_nanos().to_le_bytes());
        }
        Err(e) => debug!("no usable mtime for {name}, fingerprinting size only: {e}"),
    }
}

/// Compute a SHA-256 fingerprint of a directory based on file metadata.
///
/// Hashes each listed file's name, size and modification time. Files that
/// don't exist are skipped. Returns a hex-encoded hash string.
pub fn dir_fingerprint(dir: &Path, files: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for name in files {
        let path = dir.join(name);
        if let Ok(meta) = std::fs::metadata(&path) {
            hash_file_metadata(&mut hasher, name, &meta);
        }
    }
    format!("{:x}", hasher.finalize())
}

/// Compute a SHA-256 fingerprint by scanning a directory for matching files.
///
/// Finds files matching `prefix` and `suffix`, sorts them by name, and hashes
/// each file's name, size and modification time.
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
                hash_file_metadata(&mut hasher, &f.file_name().to_string_lossy(), &meta);
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

/// Keeps the `capacity` smallest keys offered, in a bounded max-heap.
///
/// Autocomplete indexes are scanned in alphabetical order, not by relevance,
/// so a scan that stops after N hits can drop an exact match sorted after N
/// substring hits. Offering every hit to a `BestK` keeps memory bounded by the
/// limit while guaranteeing the best-ranked keys survive.
pub struct BestK<K: Ord> {
    capacity: usize,
    heap: BinaryHeap<K>,
}

impl<K: Ord> BestK<K> {
    /// Create an empty selector keeping at most `capacity` keys.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            heap: BinaryHeap::with_capacity(capacity.saturating_add(1)),
        }
    }

    /// The worst key still kept, once the selector is full.
    ///
    /// `None` while there is room left: any key would then be accepted.
    pub fn worst_kept(&self) -> Option<&K> {
        if self.heap.len() < self.capacity {
            None
        } else {
            self.heap.peek()
        }
    }

    /// Offer a key; it is kept if there is room or it beats the current worst.
    pub fn offer(&mut self, key: K) {
        if self.capacity == 0 {
            return;
        }
        if self.heap.len() < self.capacity {
            self.heap.push(key);
        } else if self.heap.peek().is_some_and(|worst| key < *worst) {
            self.heap.pop();
            self.heap.push(key);
        }
    }

    /// The kept keys, best (smallest) first.
    pub fn into_sorted_vec(self) -> Vec<K> {
        self.heap.into_sorted_vec()
    }
}

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
    fn dir_fingerprint_changes_when_mtime_changes_at_same_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"hello").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        let set_mtime = |secs: u64| {
            file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(secs))
                .unwrap();
        };

        set_mtime(1_000);
        let fp_before = dir_fingerprint(dir.path(), &["a.txt"]);
        let glob_before = dir_fingerprint_glob(dir.path(), "a", ".txt");

        set_mtime(2_000);
        let fp_after = dir_fingerprint(dir.path(), &["a.txt"]);
        let glob_after = dir_fingerprint_glob(dir.path(), "a", ".txt");

        assert_eq!(std::fs::metadata(&path).unwrap().len(), 5);
        assert_ne!(fp_before, fp_after, "same size, new mtime must change it");
        assert_ne!(glob_before, glob_after, "glob variant must track mtime too");
    }

    #[test]
    fn best_k_keeps_smallest_keys_in_order() {
        let mut best = BestK::new(3);
        assert!(best.worst_kept().is_none());
        for key in [9, 4, 7, 1, 8, 0, 5] {
            best.offer(key);
        }
        assert_eq!(best.worst_kept(), Some(&4));
        assert_eq!(best.into_sorted_vec(), vec![0, 1, 4]);
    }

    #[test]
    fn best_k_with_zero_capacity_keeps_nothing() {
        let mut best = BestK::new(0);
        best.offer(1);
        assert!(best.into_sorted_vec().is_empty());
    }

    #[test]
    fn dir_fingerprint_glob_returns_stable_value_for_empty() {
        let dir = tempfile::tempdir().unwrap();
        let fp = dir_fingerprint_glob(dir.path(), "data-", ".csv");
        assert_eq!(fp.len(), 64);
    }

    #[test]
    fn reject_parent_traversal_refuses_dot_dot() {
        assert!(reject_parent_traversal("data/../etc/passwd".into()).is_none());
        assert_eq!(
            reject_parent_traversal("data/tiles/1/2/3.png".into()),
            Some(PathBuf::from("data/tiles/1/2/3.png"))
        );
    }

    #[test]
    fn canonical_dir_resolves_parent_components() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir(dir.path().join("a")).expect("mkdir");
        let resolved = canonical_dir(&dir.path().join("a").join(".."));
        assert!(!resolved.to_string_lossy().contains(".."));
    }

    #[test]
    fn canonical_dir_keeps_a_missing_dir_as_given() {
        let missing = Path::new("does/not/exist");
        assert_eq!(canonical_dir(missing), missing.to_path_buf());
    }
}
