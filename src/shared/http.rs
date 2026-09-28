//! Outbound HTTP client shared by the handlers that call upstream services
//! (Valhalla, the tile server).
//!
//! Building a `reqwest::Client` per call throws away its connection pool, so
//! every Valhalla request paid a fresh TCP handshake. One client per worker
//! thread keeps connections alive across requests.
//!
//! Per thread rather than process-wide: a pooled connection is driven by a
//! task on the runtime that opened it. Actix runs one runtime per worker
//! thread, and tests spin up and tear down runtimes freely, so sharing a
//! connection across runtimes could hand a request to a runtime that no
//! longer exists.

use std::time::Duration;

/// Upper bound on any upstream call. Callers needing a tighter budget
/// (pedestrian legs, tiles) override it per request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// A dead upstream should fail fast rather than hold the request open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Idle connections kept per upstream host and worker.
const POOL_MAX_IDLE_PER_HOST: usize = 32;

thread_local! {
    static CLIENT: reqwest::Client = build_client();
}

/// The HTTP client of the current worker thread. Cheap: a clone shares the
/// connection pool.
pub fn client() -> reqwest::Client {
    CLIENT.with(reqwest::Client::clone)
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_max_idle_per_host(POOL_MAX_IDLE_PER_HOST)
        .build()
        .unwrap_or_else(|e| {
            // Only fails when the TLS backend cannot initialize; the default
            // client then fails the same way on first use, with a clear error.
            tracing::warn!("Failed to build tuned HTTP client, using defaults: {e}");
            reqwest::Client::new()
        })
}
