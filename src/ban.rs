//! BAN (Base Adresse Nationale) address data loader and search index.
//!
//! Loads BAN CSV files, deduplicates at the street+postcode level,
//! and provides fuzzy autocomplete search.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::text::normalize;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A deduplicated BAN address entry (one per unique street + postcode).
#[derive(Serialize, Deserialize)]
pub struct BanEntry {
    /// Display label, e.g. "Rue de Rivoli, 75001 Paris".
    pub label: String,
    /// Normalized label for fuzzy search.
    pub name_lower: String,
    /// Representative longitude.
    pub lon: f64,
    /// Representative latitude.
    pub lat: f64,
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

        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut entries: Vec<BanEntry> = Vec::new();
        let mut total_rows: u64 = 0;

        let mut files: Vec<_> = std::fs::read_dir(ban_dir)
            .unwrap_or_else(|e| {
                info!("Cannot read BAN directory: {e}");
                // Return an empty iterator by reading /dev/null-like
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

                total_rows += 1;

                if row.nom_voie.is_empty() || row.code_postal.is_empty() {
                    continue;
                }

                let key = (row.nom_voie.clone(), row.code_postal.clone());
                if !seen.insert(key) {
                    continue; // already seen this street+postcode
                }

                let lon: f64 = match row.lon.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let lat: f64 = match row.lat.parse() {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let label = format!("{}, {} {}", row.nom_voie, row.code_postal, row.nom_commune);
                let name_lower = normalize(&label);

                entries.push(BanEntry {
                    label,
                    name_lower,
                    lon,
                    lat,
                });
            }
        }

        entries.sort_by(|a, b| a.name_lower.cmp(&b.name_lower));
        info!(
            "{} BAN addresses loaded ({} rows deduplicated)",
            entries.len(),
            total_rows
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

    fn make_test_ban() -> BanData {
        BanData {
            entries: vec![
                BanEntry {
                    label: "Rue de Rivoli, 75001 Paris".to_string(),
                    name_lower: normalize("Rue de Rivoli, 75001 Paris"),
                    lon: 2.3387,
                    lat: 48.8606,
                },
                BanEntry {
                    label: "Avenue des Champs-Élysées, 75008 Paris".to_string(),
                    name_lower: normalize("Avenue des Champs-Élysées, 75008 Paris"),
                    lon: 2.3065,
                    lat: 48.8698,
                },
                BanEntry {
                    label: "Boulevard Saint-Germain, 75005 Paris".to_string(),
                    name_lower: normalize("Boulevard Saint-Germain, 75005 Paris"),
                    lon: 2.3441,
                    lat: 48.8509,
                },
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
            entries: vec![BanEntry {
                label: "Place de la Republique, 75003 Paris".to_string(),
                name_lower: normalize("Place de la Republique, 75003 Paris"),
                lon: 2.36,
                lat: 48.87,
            }],
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
