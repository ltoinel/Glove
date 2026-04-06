//! Journey endpoints, split by transport mode.
//!
//! - [`public_transport`] — `GET /api/journeys/public_transport` (RAPTOR-based)
//! - [`walk`]             — `GET /api/journeys/walk` (Valhalla pedestrian routing)
//! - [`bike`]             — `GET /api/journeys/bike` (Valhalla bicycle routing)
//! - [`car`]              — `GET /api/journeys/car` (Valhalla auto routing)

pub mod bike;
pub mod car;
pub mod public_transport;
pub mod valhalla;
pub mod walk;

pub use bike::{__path_get_bike, get_bike};
pub use car::{__path_get_car, get_car};
pub use public_transport::{__path_get_journeys, DisplayInfo, get_journeys};
pub use walk::{__path_get_walk, get_walk};
