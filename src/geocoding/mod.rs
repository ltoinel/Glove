//! Address geocoding domain.
//!
//! - [`ban`] — Base Adresse Nationale index and address lookup
//!
//! Stop-name search lives in [`crate::transit::raptor`], not here: it
//! searches the GTFS index, not the address base.

pub mod ban;
