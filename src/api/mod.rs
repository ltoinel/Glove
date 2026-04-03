//! HTTP API handlers for the Glove journey planner.
//!
//! Split by domain:
//! - [`journeys`] — `GET /api/journeys` (route planning, Navitia-compatible)
//! - [`places`]   — `GET /api/places` (stop name autocomplete)
//! - [`status`]   — `GET /api/status` + `POST /api/reload` (engine health & hot-reload)

mod journeys;
mod places;
mod status;

pub use journeys::get_journeys;
pub use places::get_places;
pub use status::{get_status, post_reload};

use serde::Serialize;

// ---------------------------------------------------------------------------
// Shared response types used across handlers
// ---------------------------------------------------------------------------

/// A geographic coordinate.
#[derive(Debug, Serialize)]
pub struct Coord {
    pub lon: f64,
    pub lat: f64,
}

/// A reference to a stop point with name and coordinates.
#[derive(Debug, Serialize)]
pub struct StopPointRef {
    pub id: String,
    pub name: String,
    pub coord: Coord,
}

/// A place (origin, destination, or intermediate stop).
#[derive(Debug, Serialize)]
pub struct Place {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_point: Option<StopPointRef>,
}

/// A section of a journey (public transport leg or transfer).
#[derive(Debug, Serialize)]
pub struct Section {
    #[serde(rename = "type")]
    pub section_type: String,
    pub from: Place,
    pub to: Place,
    pub departure_date_time: String,
    pub arrival_date_time: String,
    pub duration: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_informations: Option<journeys::DisplayInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_date_times: Option<Vec<StopDateTime>>,
}

/// A stop visit within a public transport section.
#[derive(Debug, Serialize)]
pub struct StopDateTime {
    pub stop_point: StopPointRef,
    pub arrival_date_time: String,
    pub departure_date_time: String,
}

/// Build a [`Place`] from a GTFS [`Stop`](crate::gtfs::Stop).
pub fn make_place(stop: &crate::gtfs::Stop) -> Place {
    Place {
        id: stop.stop_id.clone(),
        name: stop.stop_name.clone(),
        stop_point: Some(make_stop_point(stop)),
    }
}

/// Build a [`StopPointRef`] from a GTFS [`Stop`](crate::gtfs::Stop).
pub fn make_stop_point(stop: &crate::gtfs::Stop) -> StopPointRef {
    StopPointRef {
        id: stop.stop_id.clone(),
        name: stop.stop_name.clone(),
        coord: Coord {
            lon: stop.stop_lon,
            lat: stop.stop_lat,
        },
    }
}
