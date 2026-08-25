//! Polling service: one task per feed, one published overlay.
//!
//! Mirrors the traffic overlay's lifetime split. Each configured feed refreshes
//! on its own interval into its own snapshot slot; after every successful poll
//! the snapshots are re-resolved against the current schedule and the resulting
//! [`RealtimeIndex`] is swapped in atomically. Readers never block and never
//! see a half-built overlay.
//!
//! The service degrades quietly: disabled in configuration, no feeds, an
//! unreachable upstream or an undecodable body all leave the router running on
//! the published schedule. Failures are recorded per feed and surfaced by
//! `GET /api/realtime/status` rather than being swallowed.

use std::sync::Arc;
use std::time::Duration;

use actix_web::web;
use arc_swap::{ArcSwap, ArcSwapOption};
use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

use super::gtfs_rt::GtfsRtSource;
use super::index::{MatchStats, RealtimeIndex, TripLookup, build_index};
use super::model::RealtimeFeed;
use super::source::{FeedError, RealtimeSource};
use crate::shared::config::{FeedConfig, FeedKind, RealtimeConfig};
use crate::shared::util::redact_query;
use crate::transit::raptor::RaptorData;

/// Last known state of one feed.
#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct FeedHealth {
    pub name: String,
    /// Wire format, e.g. `gtfs-rt`.
    pub kind: String,
    /// Endpoint with its query string redacted: it often carries the API key.
    pub url: String,
    pub refresh_secs: u64,
    /// RFC 3339 timestamp of the last successful poll.
    pub last_success: Option<String>,
    /// Why the most recent poll failed, if it did.
    pub last_error: Option<String>,
    /// Trip updates carried by the last successful poll.
    pub trip_updates: usize,
}

impl FeedHealth {
    fn new(config: &FeedConfig) -> Self {
        Self {
            name: config.name.clone(),
            kind: config.kind.to_string(),
            url: redact_query(&config.url),
            refresh_secs: config.refresh_secs,
            last_success: None,
            last_error: None,
            trip_updates: 0,
        }
    }
}

/// One feed's mutable state.
struct FeedSlot {
    health: ArcSwap<FeedHealth>,
    /// Most recent decoded snapshot; `None` until the first success.
    snapshot: ArcSwapOption<RealtimeFeed>,
}

/// The trip lookup, tagged with the GTFS load it was built from.
///
/// `POST /api/gtfs/reload` swaps the whole RAPTOR index, which invalidates
/// every `(pattern_idx, trip_idx)` pair. Comparing load timestamps rebuilds
/// the lookup exactly once after a reload.
struct CachedLookup {
    built_from: DateTime<Utc>,
    lookup: TripLookup,
}

/// Shared real-time state, published to handlers as `web::Data`.
pub struct RealtimeService {
    enabled: bool,
    feeds: Vec<FeedSlot>,
    index: ArcSwapOption<RealtimeIndex>,
    lookup: ArcSwapOption<CachedLookup>,
    published_at: ArcSwapOption<DateTime<Utc>>,
}

impl RealtimeService {
    /// A service that never polls: real-time routing is off.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            feeds: Vec::new(),
            index: ArcSwapOption::empty(),
            lookup: ArcSwapOption::empty(),
            published_at: ArcSwapOption::empty(),
        }
    }

    fn new(configs: &[FeedConfig]) -> Self {
        Self {
            enabled: true,
            feeds: configs
                .iter()
                .map(|config| FeedSlot {
                    health: ArcSwap::from_pointee(FeedHealth::new(config)),
                    snapshot: ArcSwapOption::empty(),
                })
                .collect(),
            index: ArcSwapOption::empty(),
            lookup: ArcSwapOption::empty(),
            published_at: ArcSwapOption::empty(),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The current overlay, or `None` before the first successful refresh.
    ///
    /// Callers hold the returned `Arc` for the whole request so that every
    /// iteration of a diverse search sees one consistent snapshot, even if a
    /// refresh lands mid-request.
    pub fn index(&self) -> Option<Arc<RealtimeIndex>> {
        self.index.load_full()
    }

    /// Per-feed health, in configuration order.
    pub fn health(&self) -> Vec<FeedHealth> {
        self.feeds
            .iter()
            .map(|slot| (*slot.health.load_full()).clone())
            .collect()
    }

    /// When the current overlay was published, as an RFC 3339 timestamp.
    pub fn published_at(&self) -> Option<String> {
        self.published_at.load_full().map(|at| at.to_rfc3339())
    }

    /// Matching counters from the last resolution.
    pub fn stats(&self) -> MatchStats {
        self.index().map(|index| index.stats).unwrap_or_default()
    }

    /// Number of trips currently carrying an adjustment.
    pub fn updated_trips(&self) -> usize {
        self.index().map_or(0, |index| index.updated_trips())
    }

    /// Store a feed's snapshot and re-resolve the overlay against `data`.
    fn publish(&self, slot: usize, feed: RealtimeFeed, data: &RaptorData) {
        let trip_updates = feed.trip_updates.len();
        let Some(entry) = self.feeds.get(slot) else {
            return;
        };
        entry.snapshot.store(Some(Arc::new(feed)));

        let mut health = (*entry.health.load_full()).clone();
        health.last_success = Some(Utc::now().to_rfc3339());
        health.last_error = None;
        health.trip_updates = trip_updates;
        entry.health.store(Arc::new(health));

        self.resolve(data);
    }

    /// Record a failed poll without disturbing the published overlay.
    ///
    /// The last good snapshot stays in place: stale predictions beat none at
    /// all for the length of one refresh interval, and the staleness is
    /// visible through `last_success`.
    fn record_failure(&self, slot: usize, error: &FeedError) {
        let Some(entry) = self.feeds.get(slot) else {
            return;
        };
        let mut health = (*entry.health.load_full()).clone();
        health.last_error = Some(error.to_string());
        entry.health.store(Arc::new(health));
    }

    /// Rebuild the overlay from every feed's latest snapshot.
    fn resolve(&self, data: &RaptorData) {
        let lookup = self.lookup_for(data);
        let snapshots: Vec<Arc<RealtimeFeed>> = self
            .feeds
            .iter()
            .filter_map(|slot| slot.snapshot.load_full())
            .collect();

        let index = build_index(
            data,
            &lookup.lookup,
            snapshots.iter().map(|feed| feed.as_ref()),
        );
        debug!(
            "Real-time overlay: {} trips updated, {} unmatched",
            index.updated_trips(),
            index.stats.unmatched_trips
        );
        self.index.store(Some(Arc::new(index)));
        self.published_at.store(Some(Arc::new(Utc::now())));
    }

    /// The trip lookup for the current schedule, rebuilding it after a reload.
    fn lookup_for(&self, data: &RaptorData) -> Arc<CachedLookup> {
        if let Some(cached) = self.lookup.load_full()
            && cached.built_from == data.stats.loaded_at
        {
            return cached;
        }
        let lookup = Arc::new(CachedLookup {
            built_from: data.stats.loaded_at,
            lookup: TripLookup::build(data),
        });
        info!(
            "Real-time trip lookup built ({} trips)",
            lookup.lookup.trip_count()
        );
        self.lookup.store(Some(lookup.clone()));
        lookup
    }
}

// ---------------------------------------------------------------------------
// Startup and refresh loops
// ---------------------------------------------------------------------------

/// Start one refresh task per configured feed.
///
/// Returns a disabled service when real-time is off or no feed is configured,
/// so handlers need no special case.
pub fn start(
    config: &RealtimeConfig,
    shared: Arc<ArcSwap<RaptorData>>,
) -> web::Data<RealtimeService> {
    if !config.enabled {
        return web::Data::new(RealtimeService::disabled());
    }
    if config.feeds.is_empty() {
        warn!("realtime.enabled is true but no feed is configured — disabling real-time");
        return web::Data::new(RealtimeService::disabled());
    }

    let service = web::Data::new(RealtimeService::new(&config.feeds));
    for (slot, feed) in config.feeds.iter().enumerate() {
        info!(
            "Real-time feed '{}' ({}) every {}s: {}",
            feed.name,
            feed.kind,
            feed.refresh_secs,
            redact_query(&feed.url)
        );
        spawn_refresh_loop(service.clone(), shared.clone(), slot, feed.clone());
    }
    service
}

/// Build the connector for a feed's wire format.
fn build_source(config: &FeedConfig) -> Box<dyn RealtimeSource> {
    let headers: Vec<(String, String)> = config
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();

    match config.kind {
        FeedKind::GtfsRt => Box::new(GtfsRtSource::new(
            config.name.clone(),
            config.url.clone(),
            headers,
        )),
    }
}

/// Poll one feed for the lifetime of the server.
fn spawn_refresh_loop(
    service: web::Data<RealtimeService>,
    shared: Arc<ArcSwap<RaptorData>>,
    slot: usize,
    config: FeedConfig,
) {
    actix_web::rt::spawn(async move {
        // Timeout below the refresh interval: a stalled upstream must not let
        // requests pile up on top of each other.
        let timeout = Duration::from_secs(config.timeout_secs.max(1));
        let client = match reqwest::Client::builder().timeout(timeout).build() {
            Ok(client) => client,
            Err(e) => {
                warn!(
                    "Real-time feed '{}' disabled: cannot build HTTP client: {e}",
                    config.name
                );
                return;
            }
        };

        let source = build_source(&config);
        let interval = Duration::from_secs(config.refresh_secs.max(1));

        loop {
            match source.fetch(&client).await {
                Ok(feed) => {
                    debug!(
                        "Real-time feed '{}': {} trip updates",
                        source.name(),
                        feed.trip_updates.len()
                    );
                    service.publish(slot, feed, &shared.load());
                }
                Err(e) => {
                    warn!("Real-time feed '{}' refresh failed: {e}", source.name());
                    service.record_failure(slot, &e);
                }
            }
            actix_web::rt::time::sleep(interval).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transit::realtime::model::{TripRef, TripRelationship, TripUpdate};
    use std::collections::BTreeMap;

    fn feed_config(name: &str) -> FeedConfig {
        FeedConfig {
            name: name.to_string(),
            kind: FeedKind::GtfsRt,
            url: "https://example.test/trip-updates?apikey=secret".to_string(),
            refresh_secs: 30,
            timeout_secs: 10,
            headers: BTreeMap::new(),
        }
    }

    fn cancelling_feed(trip_id: &str) -> RealtimeFeed {
        RealtimeFeed {
            trip_updates: vec![TripUpdate {
                trip: TripRef {
                    trip_id: Some(trip_id.to_string()),
                    ..Default::default()
                },
                relationship: TripRelationship::Canceled,
                ..Default::default()
            }],
            timestamp: None,
        }
    }

    #[test]
    fn a_disabled_service_publishes_no_overlay() {
        let service = RealtimeService::disabled();
        assert!(!service.enabled());
        assert!(service.index().is_none());
        assert!(service.health().is_empty());
        assert!(service.published_at().is_none());
        assert_eq!(service.updated_trips(), 0);
    }

    #[test]
    fn health_starts_empty_and_hides_the_api_key() {
        let service = RealtimeService::new(&[feed_config("idfm")]);
        let health = service.health();
        assert_eq!(health.len(), 1);
        assert_eq!(health[0].name, "idfm");
        assert_eq!(health[0].kind, "gtfs-rt");
        assert_eq!(health[0].url, "https://example.test/trip-updates?…");
        assert!(health[0].last_success.is_none());
        assert!(health[0].last_error.is_none());
    }

    #[test]
    fn publishing_a_snapshot_resolves_an_overlay_and_marks_the_feed_healthy() {
        let data = crate::transit::raptor::test_support::build_test_data();
        let service = RealtimeService::new(&[feed_config("idfm")]);

        service.publish(0, cancelling_feed("T1"), &data);

        let index = service.index().expect("an overlay must be published");
        assert_eq!(index.stats.canceled_trips, 1);
        assert_eq!(service.updated_trips(), 1);
        assert!(service.published_at().is_some());
        assert!(service.health()[0].last_success.is_some());
        assert_eq!(service.health()[0].trip_updates, 1);
    }

    #[test]
    fn a_failure_keeps_the_last_good_overlay_and_records_the_cause() {
        let data = crate::transit::raptor::test_support::build_test_data();
        let service = RealtimeService::new(&[feed_config("idfm")]);
        service.publish(0, cancelling_feed("T1"), &data);

        service.record_failure(0, &FeedError::Status(503));

        assert_eq!(
            service.index().unwrap().stats.canceled_trips,
            1,
            "a failed poll must not drop the overlay"
        );
        let health = service.health();
        assert_eq!(
            health[0].last_error.as_deref(),
            Some("upstream returned HTTP 503")
        );
        assert!(
            health[0].last_success.is_some(),
            "the earlier success stays visible so staleness can be judged"
        );
    }

    #[test]
    fn the_trip_lookup_is_rebuilt_when_the_schedule_reloads() {
        let data = crate::transit::raptor::test_support::build_test_data();
        let service = RealtimeService::new(&[feed_config("idfm")]);
        service.publish(0, cancelling_feed("T1"), &data);
        let first = service.lookup.load_full().unwrap();

        // Same schedule: the lookup is reused.
        service.publish(0, cancelling_feed("T1"), &data);
        assert!(Arc::ptr_eq(&first, &service.lookup.load_full().unwrap()));

        // A reload stamps a new load time, so the lookup must be rebuilt.
        let reloaded = crate::transit::raptor::test_support::build_test_data();
        assert_ne!(reloaded.stats.loaded_at, data.stats.loaded_at);
        service.publish(0, cancelling_feed("T1"), &reloaded);
        assert!(!Arc::ptr_eq(&first, &service.lookup.load_full().unwrap()));
    }

    #[test]
    fn several_feeds_merge_into_one_overlay() {
        let data = crate::transit::raptor::test_support::build_test_data();
        let service = RealtimeService::new(&[feed_config("a"), feed_config("b")]);

        service.publish(0, cancelling_feed("T1"), &data);
        service.publish(1, cancelling_feed("T2"), &data);

        assert_eq!(service.updated_trips(), 2);
        assert_eq!(service.index().unwrap().stats.canceled_trips, 2);
    }

    #[test]
    fn start_returns_a_disabled_service_when_no_feed_is_configured() {
        let data = crate::transit::raptor::test_support::build_test_data();
        let shared = Arc::new(ArcSwap::from_pointee(data));
        let config = RealtimeConfig {
            enabled: true,
            feeds: Vec::new(),
        };
        assert!(!start(&config, shared).enabled());
    }

    #[test]
    fn start_returns_a_disabled_service_when_realtime_is_off() {
        let data = crate::transit::raptor::test_support::build_test_data();
        let shared = Arc::new(ArcSwap::from_pointee(data));
        let config = RealtimeConfig {
            enabled: false,
            feeds: vec![feed_config("idfm")],
        };
        assert!(!start(&config, shared).enabled());
    }
}
