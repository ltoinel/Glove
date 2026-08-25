//! Cross-cutting concerns shared by every domain.
//!
//! Nothing here belongs to a single domain: pulling any of these into
//! `transit`, `geocoding` or `traffic` would force the other domains to
//! depend on it.
//!
//! - [`config`] — `config.yaml` deserialization and defaults
//! - [`text`]   — French diacritics normalization for fuzzy search
//! - [`util`]   — coordinate parsing, directory fingerprints, log redaction

pub mod config;
pub mod text;
pub mod util;
