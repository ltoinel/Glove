//! Road traffic domain.
//!
//! - [`sytadin`] — Sytadin MIF/MID geometry parsing and live segment states
//!
//! Split by lifetime: the geometry is parsed once at startup and served
//! immutable, while only the states are polled and re-published.

pub mod sytadin;
