//! Operator-authored disruptions: works, incidents, closures.
//!
//! - [`model`]   — what an operator declares (scope, severity, period)
//! - [`store`]   — the persisted catalog and its CRUD
//! - [`overlay`] — the catalog resolved against the schedule, per query
//!
//! Inside `transit` rather than beside it, for the same reason as
//! [`super::realtime`]: a disruption names stops and lines of the loaded GTFS
//! and has no meaning without it, and [`super::raptor`] reads the overlay at
//! query time.
//!
//! The split between `store` and `overlay` is a lifetime split. The catalog
//! changes when an operator edits it and must survive a restart; the overlay
//! is derived, time-dependent, and rebuilt per query.

pub mod model;
pub mod overlay;
pub mod store;
