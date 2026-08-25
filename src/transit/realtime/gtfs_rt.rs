//! GTFS-Realtime connector: decodes a `FeedMessage` into the pivot model.
//!
//! Only the `TripUpdate` branch of the specification is decoded — phase 1
//! covers delays and cancellations. `VehiclePosition` and `Alert` entities are
//! skipped as unknown fields, which costs nothing and leaves room to add them
//! without touching the reader.
//!
//! Field numbers come from `gtfs-realtime.proto` v2.0 and are spelled out as
//! constants below, because a bare `3 =>` in a match arm is unreviewable.

use futures_util::future::BoxFuture;

use super::model::{
    RealtimeFeed, StopRelationship, StopTimeUpdate, TimeUpdate, TripRef, TripRelationship,
    TripUpdate,
};
use super::protobuf::{DecodeError, Reader, WireValue};
use super::source::{FeedError, RealtimeSource, fetch_body};

// ---------------------------------------------------------------------------
// Field numbers from gtfs-realtime.proto v2.0
// ---------------------------------------------------------------------------

mod field {
    // FeedMessage
    pub const HEADER: u32 = 1;
    pub const ENTITY: u32 = 2;

    // FeedHeader
    pub const HEADER_TIMESTAMP: u32 = 3;

    // FeedEntity
    pub const ENTITY_IS_DELETED: u32 = 2;
    pub const ENTITY_TRIP_UPDATE: u32 = 3;

    // TripUpdate
    pub const TRIP_UPDATE_TRIP: u32 = 1;
    pub const TRIP_UPDATE_STOP_TIME_UPDATE: u32 = 2;
    pub const TRIP_UPDATE_TIMESTAMP: u32 = 4;
    pub const TRIP_UPDATE_DELAY: u32 = 5;

    // TripDescriptor
    pub const TRIP_ID: u32 = 1;
    pub const TRIP_START_TIME: u32 = 2;
    pub const TRIP_START_DATE: u32 = 3;
    pub const TRIP_SCHEDULE_RELATIONSHIP: u32 = 4;
    pub const TRIP_ROUTE_ID: u32 = 5;

    // TripUpdate.StopTimeUpdate
    pub const STOP_SEQUENCE: u32 = 1;
    pub const STOP_ARRIVAL: u32 = 2;
    pub const STOP_DEPARTURE: u32 = 3;
    pub const STOP_ID: u32 = 4;
    pub const STOP_SCHEDULE_RELATIONSHIP: u32 = 5;

    // TripUpdate.StopTimeEvent
    pub const EVENT_DELAY: u32 = 1;
    pub const EVENT_TIME: u32 = 2;
}

/// `TripDescriptor.ScheduleRelationship`.
fn trip_relationship(value: u64) -> TripRelationship {
    match value {
        1 => TripRelationship::Added,
        2 => TripRelationship::Unscheduled,
        // CANCELED (3) and DELETED (7) both mean "do not route over this trip".
        3 | 7 => TripRelationship::Canceled,
        6 => TripRelationship::Duplicated,
        _ => TripRelationship::Scheduled,
    }
}

/// `TripUpdate.StopTimeUpdate.ScheduleRelationship`.
fn stop_relationship(value: u64) -> StopRelationship {
    match value {
        1 => StopRelationship::Skipped,
        // NO_DATA (2) and UNSCHEDULED (3) both mean "fall back on the schedule".
        2 | 3 => StopRelationship::NoData,
        _ => StopRelationship::Scheduled,
    }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// Decode a serialized `FeedMessage`.
pub fn decode_feed(buf: &[u8]) -> Result<RealtimeFeed, DecodeError> {
    let mut reader = Reader::new(buf);
    let mut feed = RealtimeFeed::default();

    while let Some((number, value)) = reader.next_field()? {
        let Some(nested) = value.as_bytes() else {
            continue;
        };
        match number {
            field::HEADER => feed.timestamp = decode_header_timestamp(nested)?,
            field::ENTITY => {
                if let Some(update) = decode_entity(nested)? {
                    feed.trip_updates.push(update);
                }
            }
            _ => {}
        }
    }
    Ok(feed)
}

/// Pull the producer timestamp out of a `FeedHeader`.
fn decode_header_timestamp(buf: &[u8]) -> Result<Option<u64>, DecodeError> {
    let mut reader = Reader::new(buf);
    let mut timestamp = None;
    while let Some((number, value)) = reader.next_field()? {
        if number == field::HEADER_TIMESTAMP {
            timestamp = value.as_u64();
        }
    }
    Ok(timestamp)
}

/// Decode a `FeedEntity`, returning its `TripUpdate` if it carries one.
///
/// `is_deleted` may appear after the payload, so the decision is deferred to
/// the end rather than taken while walking the fields.
fn decode_entity(buf: &[u8]) -> Result<Option<TripUpdate>, DecodeError> {
    let mut reader = Reader::new(buf);
    let mut update = None;
    let mut is_deleted = false;

    while let Some((number, value)) = reader.next_field()? {
        match number {
            field::ENTITY_IS_DELETED => is_deleted = value.as_u64().is_some_and(|v| v != 0),
            field::ENTITY_TRIP_UPDATE => {
                if let Some(nested) = value.as_bytes() {
                    update = Some(decode_trip_update(nested)?);
                }
            }
            _ => {}
        }
    }
    Ok(if is_deleted { None } else { update })
}

/// Decode a `TripUpdate`.
fn decode_trip_update(buf: &[u8]) -> Result<TripUpdate, DecodeError> {
    let mut reader = Reader::new(buf);
    let mut update = TripUpdate::default();

    while let Some((number, value)) = reader.next_field()? {
        match number {
            field::TRIP_UPDATE_TRIP => {
                if let Some(nested) = value.as_bytes() {
                    let (trip, relationship) = decode_trip_descriptor(nested)?;
                    update.trip = trip;
                    update.relationship = relationship;
                }
            }
            field::TRIP_UPDATE_STOP_TIME_UPDATE => {
                if let Some(nested) = value.as_bytes() {
                    update.stop_updates.push(decode_stop_time_update(nested)?);
                }
            }
            field::TRIP_UPDATE_TIMESTAMP => update.timestamp = value.as_u64(),
            field::TRIP_UPDATE_DELAY => update.delay = value.as_i32(),
            _ => {}
        }
    }
    Ok(update)
}

/// Decode a `TripDescriptor` into its identity and its schedule relationship.
fn decode_trip_descriptor(buf: &[u8]) -> Result<(TripRef, TripRelationship), DecodeError> {
    let mut reader = Reader::new(buf);
    let mut trip = TripRef::default();
    let mut relationship = TripRelationship::Scheduled;

    while let Some((number, value)) = reader.next_field()? {
        match number {
            field::TRIP_ID => trip.trip_id = value.as_str()?.map(str::to_owned),
            field::TRIP_START_TIME => trip.start_time = value.as_str()?.map(str::to_owned),
            field::TRIP_START_DATE => trip.start_date = value.as_str()?.map(str::to_owned),
            field::TRIP_ROUTE_ID => trip.route_id = value.as_str()?.map(str::to_owned),
            field::TRIP_SCHEDULE_RELATIONSHIP => {
                if let Some(v) = value.as_u64() {
                    relationship = trip_relationship(v);
                }
            }
            _ => {}
        }
    }
    Ok((trip, relationship))
}

/// Decode a `TripUpdate.StopTimeUpdate`.
fn decode_stop_time_update(buf: &[u8]) -> Result<StopTimeUpdate, DecodeError> {
    let mut reader = Reader::new(buf);
    let mut update = StopTimeUpdate::default();

    while let Some((number, value)) = reader.next_field()? {
        match number {
            field::STOP_SEQUENCE => update.stop_sequence = value.as_u32(),
            field::STOP_ID => update.stop_id = value.as_str()?.map(str::to_owned),
            field::STOP_ARRIVAL => update.arrival = decode_stop_time_event(value)?,
            field::STOP_DEPARTURE => update.departure = decode_stop_time_event(value)?,
            field::STOP_SCHEDULE_RELATIONSHIP => {
                if let Some(v) = value.as_u64() {
                    update.relationship = stop_relationship(v);
                }
            }
            _ => {}
        }
    }
    Ok(update)
}

/// Decode a `TripUpdate.StopTimeEvent`.
///
/// Returns `None` for an event that carries neither a delay nor a time, so
/// that an empty message is not mistaken for "on time".
fn decode_stop_time_event(value: WireValue<'_>) -> Result<Option<TimeUpdate>, DecodeError> {
    let Some(buf) = value.as_bytes() else {
        return Ok(None);
    };
    let mut reader = Reader::new(buf);
    let mut event = TimeUpdate::default();

    while let Some((number, field_value)) = reader.next_field()? {
        match number {
            field::EVENT_DELAY => event.delay = field_value.as_i32(),
            field::EVENT_TIME => event.time = field_value.as_i64(),
            _ => {}
        }
    }
    Ok(if event.is_empty() { None } else { Some(event) })
}

// ---------------------------------------------------------------------------
// Source
// ---------------------------------------------------------------------------

/// Polls a GTFS-Realtime `TripUpdates` endpoint.
pub struct GtfsRtSource {
    name: String,
    url: String,
    /// Extra request headers, typically the provider's API key.
    headers: Vec<(String, String)>,
}

impl GtfsRtSource {
    pub fn new(name: String, url: String, headers: Vec<(String, String)>) -> Self {
        Self { name, url, headers }
    }
}

impl RealtimeSource for GtfsRtSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
    ) -> BoxFuture<'a, Result<RealtimeFeed, FeedError>> {
        Box::pin(async move {
            let body = fetch_body(client, &self.url, &self.headers).await?;
            decode_feed(&body).map_err(|e| FeedError::Decode(e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transit::realtime::protobuf::encoding::message;

    /// Build a `StopTimeEvent` holding a delay.
    fn stop_time_event(delay: i32) -> Vec<u8> {
        message(&[(
            field::EVENT_DELAY,
            WireValue::Varint(i64::from(delay) as u64),
        )])
    }

    /// Build a minimal `FeedMessage` around one `TripUpdate`.
    fn feed_with_trip_update(trip_update: &[u8]) -> Vec<u8> {
        let entity = message(&[(field::ENTITY_TRIP_UPDATE, WireValue::Bytes(trip_update))]);
        let header = message(&[(field::HEADER_TIMESTAMP, WireValue::Varint(1_700_000_000))]);
        message(&[
            (field::HEADER, WireValue::Bytes(&header)),
            (field::ENTITY, WireValue::Bytes(&entity)),
        ])
    }

    fn scheduled_trip(trip_id: &str) -> Vec<u8> {
        message(&[(field::TRIP_ID, WireValue::Bytes(trip_id.as_bytes()))])
    }

    #[test]
    fn decodes_a_delay_on_a_scheduled_trip() {
        let arrival = stop_time_event(180);
        let departure = stop_time_event(240);
        let stop = message(&[
            (field::STOP_SEQUENCE, WireValue::Varint(4)),
            (field::STOP_ARRIVAL, WireValue::Bytes(&arrival)),
            (field::STOP_DEPARTURE, WireValue::Bytes(&departure)),
            (field::STOP_ID, WireValue::Bytes(b"IDFM:22101")),
        ]);
        let trip = scheduled_trip("IDFM:TRIP:1");
        let trip_update = message(&[
            (field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip)),
            (field::TRIP_UPDATE_STOP_TIME_UPDATE, WireValue::Bytes(&stop)),
        ]);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();

        assert_eq!(feed.timestamp, Some(1_700_000_000));
        assert_eq!(feed.trip_updates.len(), 1);
        let update = &feed.trip_updates[0];
        assert_eq!(update.trip.trip_id.as_deref(), Some("IDFM:TRIP:1"));
        assert_eq!(update.relationship, TripRelationship::Scheduled);
        assert_eq!(update.stop_updates.len(), 1);
        let stop = &update.stop_updates[0];
        assert_eq!(stop.stop_id.as_deref(), Some("IDFM:22101"));
        assert_eq!(stop.stop_sequence, Some(4));
        assert_eq!(stop.arrival.unwrap().delay, Some(180));
        assert_eq!(stop.departure.unwrap().delay, Some(240));
        assert_eq!(stop.relationship, StopRelationship::Scheduled);
    }

    #[test]
    fn decodes_a_negative_delay_as_running_early() {
        let arrival = stop_time_event(-90);
        let stop = message(&[(field::STOP_ARRIVAL, WireValue::Bytes(&arrival))]);
        let trip = scheduled_trip("T");
        let trip_update = message(&[
            (field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip)),
            (field::TRIP_UPDATE_STOP_TIME_UPDATE, WireValue::Bytes(&stop)),
        ]);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();
        let arrival = feed.trip_updates[0].stop_updates[0].arrival.unwrap();
        assert_eq!(arrival.delay, Some(-90));
    }

    #[test]
    fn decodes_an_absolute_time_without_a_delay() {
        let arrival = message(&[(field::EVENT_TIME, WireValue::Varint(1_700_000_500))]);
        let stop = message(&[(field::STOP_ARRIVAL, WireValue::Bytes(&arrival))]);
        let trip = scheduled_trip("T");
        let trip_update = message(&[
            (field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip)),
            (field::TRIP_UPDATE_STOP_TIME_UPDATE, WireValue::Bytes(&stop)),
        ]);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();
        let arrival = feed.trip_updates[0].stop_updates[0].arrival.unwrap();
        assert_eq!(arrival.time, Some(1_700_000_500));
        assert_eq!(arrival.delay, None);
    }

    #[test]
    fn an_empty_stop_time_event_is_not_read_as_on_time() {
        let stop = message(&[(field::STOP_ARRIVAL, WireValue::Bytes(&[]))]);
        let trip = scheduled_trip("T");
        let trip_update = message(&[
            (field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip)),
            (field::TRIP_UPDATE_STOP_TIME_UPDATE, WireValue::Bytes(&stop)),
        ]);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();
        assert!(feed.trip_updates[0].stop_updates[0].arrival.is_none());
    }

    #[test]
    fn decodes_a_cancelled_trip() {
        let trip = message(&[
            (field::TRIP_ID, WireValue::Bytes(b"IDFM:TRIP:2")),
            (field::TRIP_SCHEDULE_RELATIONSHIP, WireValue::Varint(3)),
            (field::TRIP_START_DATE, WireValue::Bytes(b"20260824")),
            (field::TRIP_ROUTE_ID, WireValue::Bytes(b"IDFM:C01371")),
        ]);
        let trip_update = message(&[(field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip))]);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();
        let update = &feed.trip_updates[0];
        assert_eq!(update.relationship, TripRelationship::Canceled);
        assert_eq!(update.trip.start_date.as_deref(), Some("20260824"));
        assert_eq!(update.trip.route_id.as_deref(), Some("IDFM:C01371"));
    }

    #[test]
    fn deleted_trips_are_treated_as_cancelled() {
        assert_eq!(trip_relationship(7), TripRelationship::Canceled);
    }

    #[test]
    fn decodes_a_skipped_stop() {
        let stop = message(&[
            (field::STOP_ID, WireValue::Bytes(b"IDFM:22101")),
            (field::STOP_SCHEDULE_RELATIONSHIP, WireValue::Varint(1)),
        ]);
        let trip = scheduled_trip("T");
        let trip_update = message(&[
            (field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip)),
            (field::TRIP_UPDATE_STOP_TIME_UPDATE, WireValue::Bytes(&stop)),
        ]);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();
        assert_eq!(
            feed.trip_updates[0].stop_updates[0].relationship,
            StopRelationship::Skipped
        );
    }

    #[test]
    fn keeps_stop_updates_in_feed_order() {
        let stops: Vec<Vec<u8>> = ["A", "B", "C"]
            .iter()
            .map(|id| message(&[(field::STOP_ID, WireValue::Bytes(id.as_bytes()))]))
            .collect();
        let trip = scheduled_trip("T");
        let mut fields = vec![(field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip))];
        for stop in &stops {
            fields.push((field::TRIP_UPDATE_STOP_TIME_UPDATE, WireValue::Bytes(stop)));
        }
        let trip_update = message(&fields);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();
        let ids: Vec<_> = feed.trip_updates[0]
            .stop_updates
            .iter()
            .filter_map(|s| s.stop_id.as_deref())
            .collect();
        assert_eq!(ids, vec!["A", "B", "C"]);
    }

    #[test]
    fn deleted_entities_are_dropped() {
        let trip = scheduled_trip("T");
        let trip_update = message(&[(field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip))]);
        let entity = message(&[
            (field::ENTITY_TRIP_UPDATE, WireValue::Bytes(&trip_update)),
            (field::ENTITY_IS_DELETED, WireValue::Varint(1)),
        ]);
        let feed = decode_feed(&message(&[(field::ENTITY, WireValue::Bytes(&entity))])).unwrap();
        assert!(feed.trip_updates.is_empty());
    }

    #[test]
    fn vehicle_and_alert_entities_are_skipped_without_error() {
        // Field 4 = vehicle, field 5 = alert: neither is decoded in phase 1.
        let entity = message(&[
            (4, WireValue::Bytes(b"\x08\x01")),
            (5, WireValue::Bytes(b"\x08\x02")),
        ]);
        let feed = decode_feed(&message(&[(field::ENTITY, WireValue::Bytes(&entity))])).unwrap();
        assert!(feed.trip_updates.is_empty());
    }

    #[test]
    fn a_trip_level_delay_is_kept_as_a_fallback() {
        let trip = scheduled_trip("T");
        let trip_update = message(&[
            (field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip)),
            (field::TRIP_UPDATE_DELAY, WireValue::Varint(300i64 as u64)),
            (
                field::TRIP_UPDATE_TIMESTAMP,
                WireValue::Varint(1_700_000_042),
            ),
        ]);

        let feed = decode_feed(&feed_with_trip_update(&trip_update)).unwrap();
        assert_eq!(feed.trip_updates[0].delay, Some(300));
        assert_eq!(feed.trip_updates[0].timestamp, Some(1_700_000_042));
    }

    #[test]
    fn a_truncated_feed_is_rejected() {
        let trip = scheduled_trip("T");
        let trip_update = message(&[(field::TRIP_UPDATE_TRIP, WireValue::Bytes(&trip))]);
        let mut buf = feed_with_trip_update(&trip_update);
        buf.truncate(buf.len() - 3);
        assert!(decode_feed(&buf).is_err());
    }

    #[test]
    fn an_empty_feed_decodes_to_no_updates() {
        let feed = decode_feed(&[]).unwrap();
        assert!(feed.trip_updates.is_empty());
        assert_eq!(feed.timestamp, None);
    }
}
