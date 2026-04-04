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
