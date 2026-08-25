//! Public transport domain: schedules, routing, and live vehicle data.
//!
//! - [`gtfs`]     — GTFS CSV loading and the raw schedule model
//! - [`raptor`]   — RAPTOR index construction and journey search
//! - [`realtime`] — delays and cancellations applied over the schedule
//!
//! `realtime` sits inside this domain rather than beside it: it has no
//! meaning without the schedule it overlays, and [`raptor`] reads it at
//! query time.

pub mod gtfs;
pub mod raptor;
pub mod realtime;
