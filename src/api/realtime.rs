//! `GET /api/realtime/status` — health of the real-time feeds.
//!
//! Deliberately reports more than "up or down". A feed can answer HTTP 200
//! with a perfectly valid body and still contribute nothing, because its
//! identifiers do not match the GTFS namespace — the usual failure when
//! plugging in a new provider. The matching counters make that case visible
//! instead of leaving it indistinguishable from a network running on time.

use actix_web::{HttpResponse, get, web};
use serde::Serialize;
use utoipa::ToSchema;

use crate::transit::realtime::index::MatchStats;
use crate::transit::realtime::service::{FeedHealth, RealtimeService};

/// Real-time engine status.
#[derive(Debug, Serialize, ToSchema)]
pub struct RealtimeStatusResponse {
    /// Whether real-time routing is configured and running.
    pub enabled: bool,
    /// When the current overlay was published (RFC 3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// Trips currently carrying a delay or a cancellation.
    pub updated_trips: usize,
    /// How much of the last refresh could be applied to the schedule.
    pub matching: MatchStats,
    /// One entry per configured feed, in configuration order.
    pub feeds: Vec<FeedHealth>,
}

/// Report real-time feed health and how well the data matches the schedule.
#[utoipa::path(
    get,
    path = "/api/realtime/status",
    tag = "Realtime",
    responses(
        (status = 200, description = "Real-time engine status", body = RealtimeStatusResponse)
    )
)]
#[get("/api/realtime/status")]
pub async fn get_realtime_status(service: web::Data<RealtimeService>) -> HttpResponse {
    HttpResponse::Ok().json(RealtimeStatusResponse {
        enabled: service.enabled(),
        published_at: service.published_at(),
        updated_trips: service.updated_trips(),
        matching: service.stats(),
        feeds: service.health(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode, test};

    #[actix_web::test]
    async fn reports_disabled_when_no_feed_runs() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(RealtimeService::disabled()))
                .service(get_realtime_status),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/realtime/status")
            .to_request();
        let response = test::call_service(&app, req).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["enabled"], false);
        assert_eq!(body["updated_trips"], 0);
        assert_eq!(body["feeds"].as_array().unwrap().len(), 0);
        assert!(
            body.get("published_at").is_none(),
            "no overlay has been published yet"
        );
    }
}
