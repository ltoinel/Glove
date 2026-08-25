//! Public transport domain: schedules, routing, live data, and disruptions.
//!
//! - [`gtfs`]        — GTFS CSV loading and the raw schedule model
//! - [`raptor`]      — RAPTOR index construction and journey search
//! - [`realtime`]    — delays and cancellations published by the operator's feeds
//! - [`disruptions`] — works and closures authored by hand in the back office
//!
//! `realtime` and `disruptions` sit inside this domain rather than beside it:
//! both name stops and lines of the loaded GTFS, neither means anything
//! without it, and [`raptor`] reads both overlays at query time. They differ
//! in origin — one is polled, the other authored — not in subject.

pub mod disruptions;
pub mod gtfs;
pub mod raptor;
pub mod realtime;
