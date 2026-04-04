//! HTTP API handlers for the Glove journey planner.
//!
//! Split by domain:
//! - [`journeys`] — Journey planning by mode (`public_transport`, `walk`, `bike`)
//! - [`places`]   — `GET /api/places` (stop name autocomplete)
//! - [`status`]   — `GET /api/status` + `POST /api/reload` (engine health & hot-reload)

pub mod journeys;
pub mod metrics;
pub mod places;
pub mod status;

pub use journeys::{__path_get_bike, get_bike};
pub use journeys::{__path_get_car, get_car};
pub use journeys::{__path_get_journeys, get_journeys};
pub use journeys::{__path_get_walk, get_walk};
pub use metrics::{__path_get_metrics, get_metrics};
pub use places::{__path_get_places, get_places};
pub use status::{__path_get_status, __path_post_reload, get_status, post_reload};

use serde::Serialize;
use utoipa::ToSchema;

// ---------------------------------------------------------------------------
// Shared response types used across handlers
// ---------------------------------------------------------------------------

/// A geographic coordinate.
#[derive(Debug, Serialize, ToSchema)]
pub struct Coord {
    pub lon: f64,
    pub lat: f64,
}

/// A reference to a stop point with name and coordinates.
#[derive(Debug, Serialize, ToSchema)]
pub struct StopPointRef {
    pub id: String,
    pub name: String,
    pub coord: Coord,
}

/// A place (origin, destination, or intermediate stop).
#[derive(Debug, Serialize, ToSchema)]
pub struct Place {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_point: Option<StopPointRef>,
}

/// A section of a journey (public transport leg or transfer).
#[derive(Debug, Serialize, ToSchema)]
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
#[derive(Debug, Serialize, ToSchema)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs::Stop;

    fn test_stop() -> Stop {
        Stop {
            stop_id: "IDFM:22101".to_string(),
            stop_name: "Châtelet".to_string(),
            stop_lon: 2.347,
            stop_lat: 48.858,
            parent_station: String::new(),
        }
    }

    #[test]
    fn make_place_fields() {
        let stop = test_stop();
        let place = make_place(&stop);
        assert_eq!(place.id, "IDFM:22101");
        assert_eq!(place.name, "Châtelet");
        assert!(place.stop_point.is_some());
        let sp = place.stop_point.unwrap();
        assert_eq!(sp.coord.lon, 2.347);
        assert_eq!(sp.coord.lat, 48.858);
    }

    #[test]
    fn make_stop_point_fields() {
        let stop = test_stop();
        let sp = make_stop_point(&stop);
        assert_eq!(sp.id, "IDFM:22101");
        assert_eq!(sp.name, "Châtelet");
        assert_eq!(sp.coord.lon, 2.347);
        assert_eq!(sp.coord.lat, 48.858);
    }
}
