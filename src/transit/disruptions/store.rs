//! Persistence and CRUD for operator-authored disruptions.
//!
//! Unlike everything else the engine holds, disruptions are *authored* rather
//! than imported: they cannot be rebuilt from a source file on restart, so they
//! are written to disk. That does not reintroduce a database — the catalog is a
//! single JSON document, rewritten whole on every change. Operators author
//! tens to hundreds of disruptions, not millions, and the whole file is read
//! once at startup.
//!
//! Reads are lock-free ([`arc_swap::ArcSwap`]) because the router consults the
//! catalog on every journey query. Writes take a mutex — they are rare, and
//! serializing them is what makes read-modify-write of the id counter safe.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use super::model::{Disruption, DisruptionInput};

/// Everything persisted, in one document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Catalog {
    /// Next id to hand out. Persisted so an id is never reused after a delete,
    /// which would make an old bookmark point at an unrelated disruption.
    next_id: u64,
    pub disruptions: Vec<Disruption>,
}

impl Catalog {
    /// Find one disruption by id.
    pub fn get(&self, id: &str) -> Option<&Disruption> {
        self.disruptions.iter().find(|d| d.id == id)
    }
}

/// Why a write was refused.
#[derive(Debug)]
pub enum StoreError {
    /// No disruption carries that id.
    NotFound,
    /// The payload failed [`DisruptionInput::validate`].
    Invalid(String),
    /// The catalog could not be written to disk. The in-memory state is left
    /// untouched, so the store never diverges from what is persisted.
    Io(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "no such disruption"),
            Self::Invalid(reason) => write!(f, "{reason}"),
            Self::Io(e) => write!(f, "cannot persist disruptions: {e}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// The disruption catalog: lock-free to read, mutex-serialized to write.
pub struct DisruptionStore {
    path: PathBuf,
    /// Serializes writers. Readers never take it.
    writer: Mutex<()>,
    current: ArcSwap<Catalog>,
}

impl DisruptionStore {
    /// Load the catalog from `path`, or start empty when the file is absent.
    ///
    /// A file that exists but cannot be parsed is moved aside rather than
    /// overwritten: losing an operator's disruptions silently is worse than
    /// starting empty and saying so loudly.
    pub fn load(path: &Path) -> Self {
        let catalog = match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Catalog>(&content) {
                Ok(catalog) => {
                    info!(
                        "Loaded {} disruption(s) from {}",
                        catalog.disruptions.len(),
                        path.display()
                    );
                    catalog
                }
                Err(e) => {
                    let aside = path.with_extension("json.invalid");
                    error!(
                        "Cannot parse {}: {e}. Moving it to {} and starting with an empty catalog",
                        path.display(),
                        aside.display()
                    );
                    if let Err(e) = std::fs::rename(path, &aside) {
                        error!("Could not move the invalid catalog aside: {e}");
                    }
                    Catalog::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!(
                    "No disruption catalog at {} yet, starting empty",
                    path.display()
                );
                Catalog::default()
            }
            Err(e) => {
                error!(
                    "Cannot read {}: {e}. Starting with an empty catalog",
                    path.display()
                );
                Catalog::default()
            }
        };

        Self {
            path: path.to_path_buf(),
            writer: Mutex::new(()),
            current: ArcSwap::from_pointee(catalog),
        }
    }

    /// A store holding `disruptions` and nothing on disk, for tests.
    ///
    /// Read-only by contract: the path points at a scratch file, so a test
    /// that mutates it would touch the temp directory.
    #[cfg(test)]
    pub fn for_tests(disruptions: Vec<Disruption>) -> Self {
        Self {
            path: std::env::temp_dir().join("glove-test-disruptions.json"),
            writer: Mutex::new(()),
            current: ArcSwap::from_pointee(Catalog {
                next_id: disruptions.len() as u64,
                disruptions,
            }),
        }
    }

    /// The current catalog. Cheap: one atomic load, no copy.
    pub fn snapshot(&self) -> arc_swap::Guard<std::sync::Arc<Catalog>> {
        self.current.load()
    }

    /// Add a disruption and persist. Returns the stored form, id included.
    pub fn create(&self, input: DisruptionInput) -> Result<Disruption, StoreError> {
        input.validate().map_err(StoreError::Invalid)?;
        let now = local_now();

        self.mutate(|catalog| {
            catalog.next_id += 1;
            let disruption = Disruption {
                id: format!("d{}", catalog.next_id),
                title: input.title.trim().to_string(),
                message: input.message.trim().to_string(),
                cause: input.cause,
                severity: input.severity,
                scope: input.scope,
                period: input.period,
                created_at: now,
                updated_at: now,
            };
            catalog.disruptions.push(disruption.clone());
            Ok(disruption)
        })
    }

    /// Replace the mutable fields of an existing disruption and persist.
    pub fn update(&self, id: &str, input: DisruptionInput) -> Result<Disruption, StoreError> {
        input.validate().map_err(StoreError::Invalid)?;
        let now = local_now();

        self.mutate(|catalog| {
            let existing = catalog
                .disruptions
                .iter_mut()
                .find(|d| d.id == id)
                .ok_or(StoreError::NotFound)?;

            existing.title = input.title.trim().to_string();
            existing.message = input.message.trim().to_string();
            existing.cause = input.cause;
            existing.severity = input.severity;
            existing.scope = input.scope;
            existing.period = input.period;
            existing.updated_at = now;
            Ok(existing.clone())
        })
    }

    /// Remove a disruption and persist.
    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.mutate(|catalog| {
            let before = catalog.disruptions.len();
            catalog.disruptions.retain(|d| d.id != id);
            if catalog.disruptions.len() == before {
                return Err(StoreError::NotFound);
            }
            Ok(())
        })
    }

    /// Apply `change` to a copy of the catalog, persist it, then publish.
    ///
    /// The order matters: publishing only after a successful write is what
    /// guarantees a restart sees exactly what callers were told was stored.
    fn mutate<T>(
        &self,
        change: impl FnOnce(&mut Catalog) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let _guard = self.writer.lock().unwrap_or_else(|poisoned| {
            // A panicking writer leaves the catalog untouched (it mutates a
            // copy), so the lock is safe to reclaim.
            warn!("Disruption writer lock was poisoned, recovering");
            poisoned.into_inner()
        });

        let mut catalog = (**self.current.load()).clone();
        let outcome = change(&mut catalog)?;
        persist(&self.path, &catalog)?;
        self.current.store(std::sync::Arc::new(catalog));
        Ok(outcome)
    }
}

/// Server-local wall clock, matching how journey queries resolve "now".
fn local_now() -> chrono::NaiveDateTime {
    chrono::Local::now().naive_local()
}

/// Write the catalog through a temporary file, then rename over the target.
///
/// `rename` is atomic within a filesystem, so a crash mid-write leaves the
/// previous catalog intact rather than a truncated one.
fn persist(path: &Path, catalog: &Catalog) -> Result<(), StoreError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
    }

    let body = serde_json::to_string_pretty(catalog).map_err(|e| StoreError::Io(e.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| StoreError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| StoreError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transit::disruptions::model::{Cause, Period, Scope, Severity};

    fn at(text: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").expect("valid test time")
    }

    fn input(title: &str, stop_id: &str) -> DisruptionInput {
        DisruptionInput {
            title: title.to_string(),
            message: String::new(),
            cause: Cause::Works,
            severity: Severity::Blocking,
            scope: Scope::Stop {
                stop_id: stop_id.to_string(),
            },
            period: Period {
                starts_at: at("2026-09-01T22:00:00"),
                ends_at: None,
            },
        }
    }

    fn temp_store() -> (tempfile::TempDir, DisruptionStore) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("disruptions.json");
        let store = DisruptionStore::load(&path);
        (dir, store)
    }

    #[test]
    fn a_missing_file_starts_an_empty_catalog() {
        let (_dir, store) = temp_store();
        assert!(store.snapshot().disruptions.is_empty());
    }

    #[test]
    fn create_assigns_sequential_ids_and_timestamps() {
        let (_dir, store) = temp_store();
        let first = store.create(input("Travaux", "S1")).expect("created");
        let second = store.create(input("Incident", "S2")).expect("created");
        assert_eq!(first.id, "d1");
        assert_eq!(second.id, "d2");
        assert_eq!(first.created_at, first.updated_at);
        assert_eq!(store.snapshot().disruptions.len(), 2);
    }

    #[test]
    fn create_rejects_an_invalid_payload_without_touching_the_catalog() {
        let (_dir, store) = temp_store();
        let mut bad = input("", "S1");
        bad.title = "  ".into();
        assert!(matches!(store.create(bad), Err(StoreError::Invalid(_))));
        assert!(store.snapshot().disruptions.is_empty());
    }

    #[test]
    fn update_replaces_fields_and_bumps_updated_at() {
        let (_dir, store) = temp_store();
        let created = store.create(input("Travaux", "S1")).expect("created");

        let mut changed = input("Travaux prolongés", "S1");
        changed.severity = Severity::Info;
        let updated = store.update(&created.id, changed).expect("updated");

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.title, "Travaux prolongés");
        assert_eq!(updated.severity, Severity::Info);
        assert_eq!(updated.created_at, created.created_at);
        assert!(updated.updated_at >= created.updated_at);
    }

    #[test]
    fn update_and_delete_report_an_unknown_id() {
        let (_dir, store) = temp_store();
        assert!(matches!(
            store.update("nope", input("x", "S1")),
            Err(StoreError::NotFound)
        ));
        assert!(matches!(store.delete("nope"), Err(StoreError::NotFound)));
    }

    #[test]
    fn delete_removes_only_the_named_disruption() {
        let (_dir, store) = temp_store();
        let first = store.create(input("A", "S1")).expect("created");
        store.create(input("B", "S2")).expect("created");

        store.delete(&first.id).expect("deleted");
        let snapshot = store.snapshot();
        assert_eq!(snapshot.disruptions.len(), 1);
        assert_eq!(snapshot.disruptions[0].title, "B");
    }

    #[test]
    fn ids_are_not_reused_after_a_delete() {
        let (dir, store) = temp_store();
        let first = store.create(input("A", "S1")).expect("created");
        store.delete(&first.id).expect("deleted");
        let next = store.create(input("B", "S2")).expect("created");
        assert_eq!(next.id, "d2");

        // …and the counter survives a reload.
        let reloaded = DisruptionStore::load(&dir.path().join("disruptions.json"));
        let after = reloaded.create(input("C", "S3")).expect("created");
        assert_eq!(after.id, "d3");
    }

    #[test]
    fn the_catalog_survives_a_reload() {
        let (dir, store) = temp_store();
        store.create(input("Travaux", "S1")).expect("created");
        drop(store);

        let reloaded = DisruptionStore::load(&dir.path().join("disruptions.json"));
        let snapshot = reloaded.snapshot();
        assert_eq!(snapshot.disruptions.len(), 1);
        assert_eq!(snapshot.disruptions[0].title, "Travaux");
    }

    #[test]
    fn an_unparsable_catalog_is_moved_aside_rather_than_lost() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("disruptions.json");
        std::fs::write(&path, "{ not json").expect("write");

        let store = DisruptionStore::load(&path);
        assert!(store.snapshot().disruptions.is_empty());
        assert!(dir.path().join("disruptions.json.invalid").exists());
    }

    #[test]
    fn get_finds_by_id() {
        let (_dir, store) = temp_store();
        let created = store.create(input("Travaux", "S1")).expect("created");
        let snapshot = store.snapshot();
        assert!(snapshot.get(&created.id).is_some());
        assert!(snapshot.get("d999").is_none());
    }
}
