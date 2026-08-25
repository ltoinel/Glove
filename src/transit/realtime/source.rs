//! The connector abstraction: one trait, one snapshot, one error type.
//!
//! A connector turns whatever a provider speaks — GTFS-Realtime protobuf
//! today, SIRI tomorrow — into a [`RealtimeFeed`]. Adding a format means
//! adding one file that implements [`RealtimeSource`]; nothing downstream
//! changes.
//!
//! The trait is deliberately object-safe (`Box<dyn RealtimeSource>`) so the
//! refresh loop can drive a heterogeneous list of feeds. `async fn` in traits
//! is not object-safe, hence the explicit boxed future.

use actix_web::web::Bytes;
use futures_util::future::BoxFuture;

use super::model::RealtimeFeed;

/// Why a fetch failed. Kept coarse: the refresh loop logs it and reports the
/// last message on `GET /api/realtime/status`, it never branches on the variant.
#[derive(Debug)]
pub enum FeedError {
    /// The request never completed (DNS, TCP, TLS, timeout).
    Transport(String),
    /// The server answered with a non-success status.
    Status(u16),
    /// The body could not be read.
    Body(String),
    /// The body was read but could not be decoded into the pivot model.
    Decode(String),
}

impl std::fmt::Display for FeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport error: {e}"),
            Self::Status(code) => write!(f, "upstream returned HTTP {code}"),
            Self::Body(e) => write!(f, "cannot read body: {e}"),
            Self::Decode(e) => write!(f, "cannot decode feed: {e}"),
        }
    }
}

impl std::error::Error for FeedError {}

/// A pollable source of real-time transit data.
pub trait RealtimeSource: Send + Sync {
    /// Stable identifier used in logs and in the status endpoint.
    fn name(&self) -> &str;

    /// Fetch and decode one snapshot.
    fn fetch<'a>(
        &'a self,
        client: &'a reqwest::Client,
    ) -> BoxFuture<'a, Result<RealtimeFeed, FeedError>>;
}

/// Fetch `url` with the configured headers and return the raw body.
///
/// Shared by connectors: only the decoding step differs between formats.
pub async fn fetch_body(
    client: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
) -> Result<Bytes, FeedError> {
    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }

    let response = request
        .send()
        .await
        .map_err(|e| FeedError::Transport(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(FeedError::Status(status.as_u16()));
    }

    response
        .bytes()
        .await
        .map_err(|e| FeedError::Body(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_a_readable_cause() {
        assert_eq!(
            FeedError::Status(503).to_string(),
            "upstream returned HTTP 503"
        );
        assert_eq!(
            FeedError::Decode("truncated varint".into()).to_string(),
            "cannot decode feed: truncated varint"
        );
    }
}
