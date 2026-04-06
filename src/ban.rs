//! BAN (Base Adresse Nationale) address data loader and search index.
//!
//! Loads BAN CSV files, deduplicates at the street+postcode level,
//! and provides fuzzy autocomplete search.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::text::normalize;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single address point: street number with its exact coordinates.
#[derive(Serialize, Deserialize, Clone)]
pub struct AddressPoint {
    pub num: u32,
    pub lon: f64,
    pub lat: f64,
}

/// A deduplicated BAN address entry (one per unique street + postcode).
/// Stores all known street numbers with their exact GPS coordinates
/// for precise positioning by house number.
#[derive(Serialize, Deserialize)]
pub struct BanEntry {
    /// Display label, e.g. "Rue de Rivoli, 75001 Paris".
    pub label: String,
    /// Normalized label for fuzzy search.
    pub name_lower: String,
    /// Centroid longitude (fallback when no number is requested).
    pub lon: f64,
    /// Centroid latitude (fallback when no number is requested).
    pub lat: f64,
    /// All known address points along this street, sorted by number.
    pub points: Vec<AddressPoint>,
}

impl BanEntry {
    /// Look up the exact coordinates for a given street number.
    /// - Exact match → exact coordinates.
    /// - No exact match → interpolates between the two nearest known numbers.
    /// - No number requested or no points → centroid.
    pub fn locate(&self, number: Option<u32>) -> (f64, f64) {
        let num = match number {
            Some(n) if !self.points.is_empty() => n,
            _ => return (self.lon, self.lat),
        };

        // Exact match
        if let Ok(i) = self.points.binary_search_by_key(&num, |p| p.num) {
            return (self.points[i].lon, self.points[i].lat);
        }

        // Interpolate between the two nearest neighbors
        let pos = self.points.partition_point(|p| p.num < num);
        if pos == 0 {
            let p = &self.points[0];
            return (p.lon, p.lat);
        }
        if pos >= self.points.len() {
            let p = &self.points[self.points.len() - 1];
            return (p.lon, p.lat);
        }

        let lo = &self.points[pos - 1];
        let hi = &self.points[pos];
        let t = (num - lo.num) as f64 / (hi.num - lo.num) as f64;
        (
            lo.lon + t * (hi.lon - lo.lon),
            lo.lat + t * (hi.lat - lo.lat),
        )
    }
}

/// In-memory BAN address index.
#[derive(Serialize, Deserialize)]
pub struct BanData {
    pub entries: Vec<BanEntry>,
}

// ---------------------------------------------------------------------------
// CSV row
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BanRow {
    #[serde(default)]
    numero: String,
    #[serde(default)]
    nom_voie: String,
    #[serde(default)]
    code_postal: String,
    #[serde(default)]
    nom_commune: String,
    #[serde(default)]
    lon: String,
    #[serde(default)]
    lat: String,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

impl BanData {
    /// Load all BAN CSV files from a directory and build the search index.
    ///
    /// Files are expected to be semicolon-separated CSVs named `adresses-*.csv`.
    /// Addresses are deduplicated by `(nom_voie, code_postal)`.
    pub fn load(ban_dir: &Path) -> Self {
        if !ban_dir.exists() {
            info!("BAN directory not found: {}, skipping", ban_dir.display());
            return BanData {
                entries: Vec::new(),
            };
        }

        info!("Loading BAN data from {}", ban_dir.display());

        use std::collections::HashMap;

        struct StreetAcc {
            label: String,
            name_lower: String,
            points: Vec<AddressPoint>,
        }

        let mut streets: HashMap<(String, String), StreetAcc> = HashMap::new();

        let mut files: Vec<_> = std::fs::read_dir(ban_dir)
            .unwrap_or_else(|e| {
                info!("Cannot read BAN directory: {e}");
                std::fs::read_dir(ban_dir).unwrap()
            })
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("adresses-") && n.ends_with(".csv"))
            })
            .collect();
        files.sort_by_key(|e| e.file_name());

        for file in &files {
            let path = file.path();
            info!("Loading {}", path.display());

            let mut reader = match csv::ReaderBuilder::new()
                .delimiter(b';')
                .flexible(true)
                .from_path(&path)
            {
                Ok(r) => r,
                Err(e) => {
                    info!("Failed to open {}: {e}", path.display());
                    continue;
                }
            };

            for result in reader.deserialize::<BanRow>() {
                let row = match result {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                if row.nom_voie.is_empty() || row.code_postal.is_empty() {
                    continue;
                }

                let lon: f64 = match row.lon.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let lat: f64 = match row.lat.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let num: u32 = row.numero.parse().unwrap_or(0);
                let key = (row.nom_voie.clone(), row.code_postal.clone());

                streets
                    .entry(key)
                    .or_insert_with(|| {
                        let label =
                            format!("{}, {} {}", row.nom_voie, row.code_postal, row.nom_commune);
                        let name_lower = normalize(&label);
                        StreetAcc {
                            label,
                            name_lower,
                            points: Vec::new(),
                        }
                    })
                    .points
                    .push(AddressPoint { num, lon, lat });
            }
        }

        let total_points: usize = streets.values().map(|s| s.points.len()).sum();

        let mut entries: Vec<BanEntry> = streets
            .into_values()
            .map(|mut acc| {
                // Sort points by street number for binary search
                acc.points.sort_by_key(|p| p.num);
                acc.points.dedup_by_key(|p| p.num);

                // Centroid as fallback (when no number requested)
                let (sum_lon, sum_lat) = acc
                    .points
                    .iter()
                    .fold((0.0, 0.0), |(sl, sa), p| (sl + p.lon, sa + p.lat));
                let n = acc.points.len().max(1) as f64;

                BanEntry {
                    label: acc.label,
                    name_lower: acc.name_lower,
                    lon: sum_lon / n,
                    lat: sum_lat / n,
                    points: acc.points,
                }
            })
            .collect();

        entries.sort_by(|a, b| a.name_lower.cmp(&b.name_lower));
        info!(
            "{} BAN streets loaded ({} address points, {:.1} MB est.)",
            entries.len(),
            total_points,
            total_points as f64 * 20.0 / 1_048_576.0
        );

        BanData { entries }
    }

    // -----------------------------------------------------------------------
    // Cache persistence
    // -----------------------------------------------------------------------

    /// Compute a SHA-256 fingerprint of the BAN directory based on file sizes.
    pub fn fingerprint(ban_dir: &Path) -> String {
        let mut hasher = Sha256::new();
        if let Ok(entries) = std::fs::read_dir(ban_dir) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.starts_with("adresses-") && n.ends_with(".csv"))
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

    /// Save the BAN index to a binary cache file.
    pub fn save(
        &self,
        cache_dir: &Path,
        fingerprint: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(cache_dir)?;
        let bin_path = cache_dir.join("ban.bin");
        let fp_path = cache_dir.join("ban.fingerprint");

        let encoded = bincode::serialize(self)?;
        std::fs::write(&bin_path, &encoded)?;
        std::fs::write(&fp_path, fingerprint)?;

        info!(
            "BAN index saved to {} ({:.1} MB)",
            bin_path.display(),
            encoded.len() as f64 / 1_048_576.0
        );
        Ok(())
    }

    /// Load the BAN index from cache if the fingerprint matches.
    pub fn load_cached(cache_dir: &Path, fingerprint: &str) -> Option<Self> {
        let bin_path = cache_dir.join("ban.bin");
        let fp_path = cache_dir.join("ban.fingerprint");

        let cached_fp = std::fs::read_to_string(&fp_path).ok()?;
        if cached_fp.trim() != fingerprint {
            info!("BAN cache fingerprint mismatch, rebuilding");
            return None;
        }

        let bytes = std::fs::read(&bin_path).ok()?;
        match bincode::deserialize(&bytes) {
            Ok(data) => {
                info!("BAN index loaded from cache ({})", bin_path.display());
                Some(data)
            }
            Err(e) => {
                info!("BAN cache corrupted, rebuilding: {e}");
                None
            }
        }
    }

    // -----------------------------------------------------------------------
    // Search
    // -----------------------------------------------------------------------

    /// Search addresses by name for autocomplete.
    ///
    /// Returns up to `limit` results ranked by relevance:
    /// exact match > prefix > word-prefix > substring.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&BanEntry> {
        if query.is_empty() {
            return Vec::new();
        }

        let q = normalize(query);
        let q_words: Vec<&str> = q.split_whitespace().collect();
        let mut results: Vec<(&BanEntry, usize)> = Vec::new();

        for entry in &self.entries {
            let rank = if entry.name_lower == q {
                0
            } else if entry.name_lower.starts_with(&q) {
                1
            } else if entry
                .name_lower
                .split_whitespace()
                .any(|w| w.starts_with(&q))
            {
                2
            } else if entry.name_lower.contains(&q) {
                3
            } else if q_words.len() >= 2 && {
                // Multi-word matching: all non-numeric query words must match
                // as prefix of some entry word. Numbers (street numbers) are
                // ignored since BAN labels don't include them.
                let alpha_words: Vec<&&str> = q_words
                    .iter()
                    .filter(|w| !w.chars().all(|c| c.is_ascii_digit() || c == ','))
                    .collect();
                alpha_words.len() >= 2
                    && alpha_words.iter().all(|qw| {
                        entry
                            .name_lower
                            .split_whitespace()
                            .any(|ew| ew.starts_with(**qw))
                    })
            } {
                4
            } else {
                continue;
            };

            results.push((entry, rank));

            if results.len() >= limit * 10 {
                break;
            }
        }

        results.sort_by_key(|r| (r.1, r.0.label.len()));
        results.truncate(limit);
        results.into_iter().map(|(entry, _)| entry).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(label: &str, lon: f64, lat: f64) -> BanEntry {
        BanEntry {
            label: label.to_string(),
            name_lower: normalize(label),
            lon,
            lat,
            points: vec![
                AddressPoint {
                    num: 1,
                    lon: lon - 0.005,
                    lat: lat - 0.001,
                },
                AddressPoint { num: 100, lon, lat },
                AddressPoint {
                    num: 200,
                    lon: lon + 0.005,
                    lat: lat + 0.001,
                },
            ],
        }
    }

    fn make_test_ban() -> BanData {
        BanData {
            entries: vec![
                make_entry("Rue de Rivoli, 75001 Paris", 2.3387, 48.8606),
                make_entry("Avenue des Champs-Élysées, 75008 Paris", 2.3065, 48.8698),
                make_entry("Boulevard Saint-Germain, 75005 Paris", 2.3441, 48.8509),
            ],
        }
    }

    // -----------------------------------------------------------------------
    // search
    // -----------------------------------------------------------------------

    #[test]
    fn search_empty_query() {
        let ban = make_test_ban();
        assert!(ban.search("", 10).is_empty());
    }

    #[test]
    fn search_exact_match() {
        let ban = make_test_ban();
        let results = ban.search("rue de rivoli, 75001 paris", 10);
        assert!(!results.is_empty());
        assert!(results[0].label.contains("Rivoli"));
    }

    #[test]
    fn search_prefix() {
        let ban = make_test_ban();
        let results = ban.search("avenue", 10);
        assert!(!results.is_empty());
        assert!(results[0].label.contains("Avenue"));
    }

    #[test]
    fn search_substring() {
        let ban = make_test_ban();
        let results = ban.search("Rivoli", 10);
        assert!(!results.is_empty());
    }

    #[test]
    fn search_diacritics() {
        let ban = make_test_ban();
        // "elysees" should match "Élysées" via normalization
        let results = ban.search("elysees", 10);
        assert!(!results.is_empty());
        assert!(results[0].label.contains("Champs"));
    }

    #[test]
    fn search_no_match() {
        let ban = make_test_ban();
        let results = ban.search("zzzznonexistent", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn search_respects_limit() {
        let ban = make_test_ban();
        let results = ban.search("paris", 1);
        assert_eq!(results.len(), 1);
    }

    // -----------------------------------------------------------------------
    // load from directory
    // -----------------------------------------------------------------------

    #[test]
    fn load_missing_dir() {
        let ban = BanData::load(Path::new("/tmp/glove_test_nonexistent_ban_dir"));
        assert!(ban.entries.is_empty());
    }

    #[test]
    fn load_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let ban = BanData::load(dir.path());
        assert!(ban.entries.is_empty());
    }

    #[test]
    fn load_csv_file() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("adresses-75.csv");
        std::fs::write(
            &csv_path,
            "id;nom_voie;code_postal;nom_commune;lon;lat\n\
             1;Rue de Rivoli;75001;Paris;2.3387;48.8606\n\
             2;Rue de Rivoli;75001;Paris;2.3388;48.8607\n\
             3;Avenue Montaigne;75008;Paris;2.3025;48.8667\n",
        )
        .unwrap();
        let ban = BanData::load(dir.path());
        // 2 unique streets (Rue de Rivoli deduplicated)
        assert_eq!(ban.entries.len(), 2);
    }

    // -----------------------------------------------------------------------
    // cache persistence
    // -----------------------------------------------------------------------

    #[test]
    fn save_and_load_cache() {
        let ban = make_test_ban();
        let dir = tempfile::tempdir().unwrap();
        ban.save(dir.path(), "test_fp").unwrap();

        let loaded = BanData::load_cached(dir.path(), "test_fp");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().entries.len(), 3);
    }

    #[test]
    fn load_cache_wrong_fingerprint() {
        let ban = make_test_ban();
        let dir = tempfile::tempdir().unwrap();
        ban.save(dir.path(), "fp1").unwrap();
        assert!(BanData::load_cached(dir.path(), "fp2").is_none());
    }

    #[test]
    fn load_cache_no_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(BanData::load_cached(dir.path(), "fp").is_none());
    }

    // -----------------------------------------------------------------------
    // fingerprint
    // -----------------------------------------------------------------------

    #[test]
    fn fingerprint_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let fp = BanData::fingerprint(dir.path());
        assert_eq!(fp.len(), 64); // SHA-256 hex
    }

    #[test]
    fn fingerprint_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("adresses-75.csv");
        std::fs::write(&csv, "header\nrow1\n").unwrap();
        let fp1 = BanData::fingerprint(dir.path());
        let fp2 = BanData::fingerprint(dir.path());
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fingerprint_nonexistent_dir() {
        let fp = BanData::fingerprint(Path::new("/nonexistent"));
        assert_eq!(fp.len(), 64);
    }

    #[test]
    fn load_csv_skips_empty_fields() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("adresses-75.csv");
        // Rows with empty nom_voie or code_postal should be skipped
        std::fs::write(
            &csv_path,
            "id;nom_voie;code_postal;nom_commune;lon;lat\n\
             1;;75001;Paris;2.33;48.86\n\
             2;Rue Test;;Paris;2.33;48.86\n\
             3;Rue Valid;75001;Paris;2.33;48.86\n",
        )
        .unwrap();
        let ban = BanData::load(dir.path());
        assert_eq!(ban.entries.len(), 1);
        assert!(ban.entries[0].label.contains("Rue Valid"));
    }

    #[test]
    fn load_csv_skips_invalid_coords() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("adresses-75.csv");
        std::fs::write(
            &csv_path,
            "id;nom_voie;code_postal;nom_commune;lon;lat\n\
             1;Rue A;75001;Paris;not_a_number;48.86\n\
             2;Rue B;75001;Paris;2.33;not_a_number\n\
             3;Rue C;75001;Paris;2.33;48.86\n",
        )
        .unwrap();
        let ban = BanData::load(dir.path());
        assert_eq!(ban.entries.len(), 1);
    }

    #[test]
    fn load_csv_ignores_non_adresses_files() {
        let dir = tempfile::tempdir().unwrap();
        // This file should be ignored (wrong prefix)
        std::fs::write(
            dir.path().join("other.csv"),
            "id;nom_voie;code_postal;nom_commune;lon;lat\n1;Rue X;75001;Paris;2.33;48.86\n",
        )
        .unwrap();
        let ban = BanData::load(dir.path());
        assert!(ban.entries.is_empty());
    }

    #[test]
    fn load_cache_corrupted_data() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ban.fingerprint"), "fp1").unwrap();
        std::fs::write(dir.path().join("ban.bin"), b"corrupted").unwrap();
        let loaded = BanData::load_cached(dir.path(), "fp1");
        assert!(loaded.is_none());
    }

    #[test]
    fn search_word_prefix() {
        let ban = BanData {
            entries: vec![make_entry(
                "Place de la Republique, 75003 Paris",
                2.36,
                48.87,
            )],
        };
        // "rep" should match via word-prefix on "republique"
        let results = ban.search("rep", 10);
        assert!(!results.is_empty());
    }

    #[test]
    fn fingerprint_changes_on_file_modification() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("adresses-75.csv");
        std::fs::write(&csv, "header\nrow1\n").unwrap();
        let fp1 = BanData::fingerprint(dir.path());
        std::fs::write(&csv, "header\nrow1\nrow2\n").unwrap();
        let fp2 = BanData::fingerprint(dir.path());
        assert_ne!(fp1, fp2);
    }
}
