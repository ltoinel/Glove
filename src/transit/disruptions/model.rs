//! What an operator declares when part of the network stops working.
//!
//! The vocabulary is deliberately operator-facing rather than GTFS-facing: a
//! disruption names a *stop*, a *line* or a *section of line*, because that is
//! how works and incidents are announced. Turning those names into pattern and
//! stop indices is [`super::overlay`]'s job, not this module's.
//!
//! Times are naive local date-times, matching the rest of the engine: journey
//! queries resolve "now" with [`chrono::Local::now`] and GTFS service days are
//! local calendar days. Storing UTC here would mean converting on every
//! comparison, for a planner that serves one timezone.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How the router must treat a disruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The object is unusable: the router removes it from the graph.
    #[default]
    Blocking,
    /// The object still works. Journeys touching it carry the message, and
    /// nothing is excluded — a platform change or a crowding notice.
    Info,
}

/// Why the network is disrupted. Free-form causes were rejected on purpose:
/// a closed list is what lets the portal group and colour them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    /// Planned engineering work.
    #[default]
    Works,
    /// Unplanned incident (signalling failure, trespasser, breakdown).
    Incident,
    /// Industrial action.
    Strike,
    /// Crowd management around an event.
    Event,
    /// Weather-related.
    Weather,
    /// Anything else; the message carries the detail.
    Other,
}

/// The part of the network a disruption applies to.
///
/// `stop_id` and `route_id` are GTFS identifiers as they appear in the loaded
/// dataset. A `stop_id` may name a parent station: [`super::overlay`] expands
/// it to the station's children, so an operator can close "Châtelet" without
/// listing its twelve platforms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scope {
    /// One stop or station, neutralized entirely: no boarding, no alighting,
    /// no transfer. Vehicles still run through it.
    Stop { stop_id: String },
    /// A whole line, in both directions.
    Line { route_id: String },
    /// The stretch of a line between two stops, in both directions.
    ///
    /// Riding through the section is cut; the line keeps working on either
    /// side of it, which is what "interruption entre A et B" means on the
    /// ground.
    LineSection {
        route_id: String,
        from_stop_id: String,
        to_stop_id: String,
    },
}

impl Scope {
    /// Short machine-readable discriminant, used for filtering and in the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Stop { .. } => "stop",
            Self::Line { .. } => "line",
            Self::LineSection { .. } => "line_section",
        }
    }
}

/// When a disruption applies.
///
/// `ends_at` is optional by design: an ongoing incident with no announced
/// resumption is the common case, and forcing operators to invent an end time
/// would either cut the disruption early or push it absurdly far out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Period {
    /// Local date-time the disruption starts, `YYYY-MM-DDTHH:MM:SS`.
    #[schema(value_type = String, format = DateTime, example = "2026-09-01T22:00:00")]
    pub starts_at: chrono::NaiveDateTime,
    /// Local date-time it ends. `null` means ongoing, no resumption announced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, format = DateTime, example = "2026-09-08T04:30:00")]
    pub ends_at: Option<chrono::NaiveDateTime>,
}

impl Period {
    /// Whether `instant` falls inside the period.
    ///
    /// Half-open on the end so a disruption ending at 05:00 does not also
    /// disrupt the 05:00 departure.
    pub fn covers(&self, instant: chrono::NaiveDateTime) -> bool {
        instant >= self.starts_at && self.ends_at.is_none_or(|end| instant < end)
    }

    /// Reject a period that ends before it starts. Returns the reason so the
    /// API can hand it back to the caller verbatim.
    pub fn validate(&self) -> Result<(), String> {
        match self.ends_at {
            Some(end) if end <= self.starts_at => {
                Err("ends_at must be strictly after starts_at".to_string())
            }
            _ => Ok(()),
        }
    }
}

/// A disruption as stored and as returned by the API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Disruption {
    /// Server-assigned, stable across updates and never reused after a delete.
    pub id: String,
    /// One-line headline, shown in journey results.
    pub title: String,
    /// Longer explanation. May be empty.
    #[serde(default)]
    pub message: String,
    pub cause: Cause,
    pub severity: Severity,
    pub scope: Scope,
    #[serde(flatten)]
    pub period: Period,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: chrono::NaiveDateTime,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: chrono::NaiveDateTime,
}

/// The mutable fields of a disruption, as accepted by `POST` and `PUT`.
///
/// Separate from [`Disruption`] so a client cannot set `id`, `created_at` or
/// `updated_at`: those belong to the store.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DisruptionInput {
    pub title: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub cause: Cause,
    #[serde(default)]
    pub severity: Severity,
    pub scope: Scope,
    #[serde(flatten)]
    pub period: Period,
}

impl DisruptionInput {
    /// Check everything the store cannot fix up itself.
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("title must not be empty".to_string());
        }
        match &self.scope {
            Scope::Stop { stop_id } if stop_id.trim().is_empty() => {
                return Err("scope.stop_id must not be empty".to_string());
            }
            Scope::Line { route_id } if route_id.trim().is_empty() => {
                return Err("scope.route_id must not be empty".to_string());
            }
            Scope::LineSection {
                route_id,
                from_stop_id,
                to_stop_id,
            } => {
                if route_id.trim().is_empty() {
                    return Err("scope.route_id must not be empty".to_string());
                }
                if from_stop_id.trim().is_empty() || to_stop_id.trim().is_empty() {
                    return Err(
                        "scope.from_stop_id and scope.to_stop_id must not be empty".to_string()
                    );
                }
                if from_stop_id == to_stop_id {
                    return Err("scope.from_stop_id and scope.to_stop_id must differ".to_string());
                }
            }
            _ => {}
        }
        self.period.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").expect("valid test time")
    }

    fn period(start: &str, end: Option<&str>) -> Period {
        Period {
            starts_at: at(start),
            ends_at: end.map(at),
        }
    }

    #[test]
    fn open_ended_period_covers_everything_after_its_start() {
        let p = period("2026-09-01T22:00:00", None);
        assert!(!p.covers(at("2026-09-01T21:59:59")));
        assert!(p.covers(at("2026-09-01T22:00:00")));
        assert!(p.covers(at("2030-01-01T00:00:00")));
    }

    #[test]
    fn closed_period_is_half_open_at_the_end() {
        let p = period("2026-09-05T00:00:00", Some("2026-09-08T04:30:00"));
        assert!(p.covers(at("2026-09-08T04:29:59")));
        assert!(!p.covers(at("2026-09-08T04:30:00")));
    }

    #[test]
    fn a_period_ending_before_it_starts_is_rejected() {
        assert!(
            period("2026-09-05T10:00:00", Some("2026-09-05T09:00:00"))
                .validate()
                .is_err()
        );
        assert!(
            period("2026-09-05T10:00:00", Some("2026-09-05T10:00:00"))
                .validate()
                .is_err()
        );
        assert!(period("2026-09-05T10:00:00", None).validate().is_ok());
    }

    fn input(scope: Scope) -> DisruptionInput {
        DisruptionInput {
            title: "Travaux".to_string(),
            message: String::new(),
            cause: Cause::Works,
            severity: Severity::Blocking,
            scope,
            period: period("2026-09-01T22:00:00", None),
        }
    }

    #[test]
    fn input_rejects_an_empty_title() {
        let mut i = input(Scope::Stop {
            stop_id: "S1".into(),
        });
        i.title = "   ".into();
        assert!(i.validate().is_err());
    }

    #[test]
    fn input_rejects_a_section_with_identical_endpoints() {
        let i = input(Scope::LineSection {
            route_id: "R1".into(),
            from_stop_id: "S1".into(),
            to_stop_id: "S1".into(),
        });
        assert!(i.validate().is_err());
    }

    #[test]
    fn input_accepts_a_well_formed_section() {
        let i = input(Scope::LineSection {
            route_id: "R1".into(),
            from_stop_id: "S1".into(),
            to_stop_id: "S2".into(),
        });
        assert!(i.validate().is_ok());
    }

    #[test]
    fn scope_kind_is_stable_for_the_api() {
        assert_eq!(
            Scope::Stop {
                stop_id: "S".into()
            }
            .kind(),
            "stop"
        );
        assert_eq!(
            Scope::Line {
                route_id: "R".into()
            }
            .kind(),
            "line"
        );
        assert_eq!(
            Scope::LineSection {
                route_id: "R".into(),
                from_stop_id: "A".into(),
                to_stop_id: "B".into(),
            }
            .kind(),
            "line_section"
        );
    }

    #[test]
    fn period_serializes_flat_and_omits_a_null_end() {
        let json = serde_json::to_string(&period("2026-09-01T22:00:00", None)).expect("serialize");
        assert_eq!(json, r#"{"starts_at":"2026-09-01T22:00:00"}"#);
    }
}
