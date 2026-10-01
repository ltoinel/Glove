//! Service days: whether anything runs today, how long the feed stays valid,
//! and services that can never run or are never used.

use chrono::{Datelike, NaiveDate, Weekday};
use rustc_hash::FxHashSet;

use super::{Category, Collector, Ctx, Finding, Rule, Severity};
use crate::transit::gtfs::{Calendar, GtfsData};

/// Below this many days of remaining validity, the feed expiry is a warning.
const EXPIRY_WARNING_DAYS: i64 = 7;

/// `calendar_dates.exception_type` for a service added on a date.
const SERVICE_ADDED: u8 = 1;
/// `calendar_dates.exception_type` for a service removed on a date.
const SERVICE_REMOVED: u8 = 2;

pub(super) const RULES: &[Rule] = &[
    Rule {
        id: "calendar_inverted",
        title: "Calendar periods start before they end",
        category: Category::Calendar,
        run: calendar_inverted,
    },
    Rule {
        id: "feed_validity",
        title: "The feed is valid today and for the coming days",
        category: Category::Calendar,
        run: feed_validity,
    },
    Rule {
        id: "services_today",
        title: "Services run today",
        category: Category::Calendar,
        run: services_today,
    },
    Rule {
        id: "services_never_run",
        title: "Every service runs at least one day",
        category: Category::Calendar,
        run: services_never_run,
    },
    Rule {
        id: "services_unused",
        title: "Every service is used by a trip",
        category: Category::Calendar,
        run: services_unused,
    },
];

fn parse_date(raw: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw.trim(), "%Y%m%d").ok()
}

/// First and last day any service is defined on: calendar periods and
/// dates added by exceptions.
pub(super) fn service_period(gtfs: &GtfsData) -> Option<(NaiveDate, NaiveDate)> {
    let calendar_days = gtfs
        .calendars
        .values()
        .flat_map(|c| [parse_date(&c.start_date), parse_date(&c.end_date)]);
    let added_days = gtfs
        .calendar_dates
        .iter()
        .filter(|cd| cd.exception_type == SERVICE_ADDED)
        .map(|cd| parse_date(&cd.date));
    let days: Vec<NaiveDate> = calendar_days.chain(added_days).flatten().collect();
    Some((*days.iter().min()?, *days.iter().max()?))
}

/// Whether the weekly pattern of `cal` runs on `date` (exceptions aside).
fn runs_on(cal: &Calendar, date: NaiveDate) -> bool {
    let in_period = matches!(
        (parse_date(&cal.start_date), parse_date(&cal.end_date)),
        (Some(start), Some(end)) if start <= date && date <= end
    );
    let flag = match date.weekday() {
        Weekday::Mon => cal.monday,
        Weekday::Tue => cal.tuesday,
        Weekday::Wed => cal.wednesday,
        Weekday::Thu => cal.thursday,
        Weekday::Fri => cal.friday,
        Weekday::Sat => cal.saturday,
        Weekday::Sun => cal.sunday,
    };
    in_period && flag == 1
}

fn calendar_inverted(ctx: &Ctx) -> Vec<Finding> {
    let mut calendars: Vec<&Calendar> = ctx.gtfs.calendars.values().collect();
    calendars.sort_unstable_by(|a, b| a.service_id.cmp(&b.service_id));
    let mut c = Collector::default();
    for cal in calendars {
        if cal.start_date > cal.end_date {
            c.push(|| format!("{}: {} > {}", cal.service_id, cal.start_date, cal.end_date));
        }
    }
    c.finding(Severity::Error, "Calendar start_date is after end_date")
        .into_iter()
        .collect()
}

fn feed_validity(ctx: &Ctx) -> Vec<Finding> {
    let today = ctx.input.today;
    let finding = |severity, message: String, samples: Vec<String>| Finding {
        severity,
        message,
        count: 1,
        samples,
    };
    let Some((start, end)) = service_period(ctx.gtfs) else {
        return vec![finding(
            Severity::Error,
            "No service day is defined (calendar and calendar_dates are empty)".into(),
            vec![],
        )];
    };
    let period = vec![format!("{start} → {end}")];
    let days_left = (end - today).num_days();
    let (severity, message) = if end < today {
        (Severity::Error, format!("The feed expired on {end}"))
    } else if start > today {
        (
            Severity::Warning,
            format!("The feed is not in force before {start}"),
        )
    } else if days_left < EXPIRY_WARNING_DAYS {
        (
            Severity::Warning,
            format!("The feed expires in {days_left} day(s), on {end}"),
        )
    } else {
        (
            Severity::Info,
            format!("The feed is valid until {end} ({days_left} days left)"),
        )
    };
    vec![finding(severity, message, period)]
}

/// Services running on the reference day: the weekly pattern, plus the
/// dates added and minus the dates removed by `calendar_dates`.
fn services_today(ctx: &Ctx) -> Vec<Finding> {
    let today = ctx.input.today;
    let today_raw = today.format("%Y%m%d").to_string();
    let mut active: FxHashSet<&str> = ctx
        .gtfs
        .calendars
        .values()
        .filter(|cal| runs_on(cal, today))
        .map(|cal| cal.service_id.as_str())
        .collect();
    for cd in ctx
        .gtfs
        .calendar_dates
        .iter()
        .filter(|cd| cd.date == today_raw)
    {
        match cd.exception_type {
            SERVICE_ADDED => {
                active.insert(&cd.service_id);
            }
            SERVICE_REMOVED => {
                active.remove(cd.service_id.as_str());
            }
            _ => {}
        }
    }
    let running_trips = ctx
        .gtfs
        .trips
        .values()
        .filter(|t| active.contains(t.service_id.as_str()))
        .count();
    if running_trips == 0 {
        return vec![Finding {
            severity: Severity::Error,
            message: format!("No trip runs on {today}: journeys cannot be planned today"),
            count: 0,
            samples: vec![],
        }];
    }
    vec![Finding {
        severity: Severity::Info,
        message: format!(
            "{} service(s) and {running_trips} trip(s) run on {today}",
            active.len()
        ),
        count: running_trips,
        samples: vec![],
    }]
}

/// A weekly pattern with no day set and no date added never runs.
fn services_never_run(ctx: &Ctx) -> Vec<Finding> {
    let added: FxHashSet<&str> = ctx
        .gtfs
        .calendar_dates
        .iter()
        .filter(|cd| cd.exception_type == SERVICE_ADDED)
        .map(|cd| cd.service_id.as_str())
        .collect();
    let mut calendars: Vec<&Calendar> = ctx.gtfs.calendars.values().collect();
    calendars.sort_unstable_by(|a, b| a.service_id.cmp(&b.service_id));
    let mut c = Collector::default();
    for cal in calendars {
        let no_weekday = [
            cal.monday,
            cal.tuesday,
            cal.wednesday,
            cal.thursday,
            cal.friday,
            cal.saturday,
            cal.sunday,
        ]
        .iter()
        .all(|&day| day == 0);
        if no_weekday && !added.contains(cal.service_id.as_str()) {
            c.push(|| cal.service_id.clone());
        }
    }
    c.finding(
        Severity::Warning,
        "Services with no weekday set and no date added: they never run",
    )
    .into_iter()
    .collect()
}

fn services_unused(ctx: &Ctx) -> Vec<Finding> {
    let used: FxHashSet<&str> = ctx
        .gtfs
        .trips
        .values()
        .map(|t| t.service_id.as_str())
        .collect();
    let defined: FxHashSet<&str> = ctx
        .gtfs
        .calendars
        .keys()
        .map(String::as_str)
        .chain(
            ctx.gtfs
                .calendar_dates
                .iter()
                .map(|cd| cd.service_id.as_str()),
        )
        .collect();
    let mut unused: Vec<&str> = defined.difference(&used).copied().collect();
    unused.sort_unstable();
    let mut c = Collector::default();
    for service_id in unused {
        c.push(|| service_id.to_string());
    }
    c.finding(Severity::Info, "Services defined but used by no trip")
        .into_iter()
        .collect()
}
