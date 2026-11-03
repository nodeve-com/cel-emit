//! `rows.cel` written by hand: one flat row per option in an answer.
//!
//! Needs `serde` (with `derive`), `serde_json`, `chrono`, `chrono-tz`, and `jiff`.

use chrono::{DateTime, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use jiff::SpanRelativeTo;
use serde::{Deserialize, Serialize};

type Error = Box<dyn std::error::Error>;

/// Reads an answer from JSON.
pub fn answer(text: &str) -> Result<Answer, Error> {
    Ok(serde_json::from_str(text)?)
}

/// An answer's rows, as JSON.
pub fn rows(answer: &Answer) -> Result<String, Error> {
    let location = |id: &str| {
        answer
            .locations
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| format!("no location {id}"))
    };

    let mut rows = Vec::new();
    for option in &answer.options {
        let leg = option.legs.first().ok_or("an option without legs")?;
        let from = location(&leg.location.id)?;
        let to = location(&leg.end_location.id)?;
        let product = answer
            .products
            .iter()
            .find(|p| p.id == option.product_id)
            .ok_or_else(|| format!("no product {}", option.product_id))?;
        let period = answer
            .products
            .iter()
            .find(|p| {
                p.id == option.product_id && p.multi_usage_rules.active_period.kind == "period"
            })
            .ok_or_else(|| format!("product {} has no active period", option.product_id))?;

        let depart_local = format!("{}T{}", option.start_date, option.start_time);
        let arrive_local = format!("{}T{}", option.end_date, option.end_time);
        let depart_utc = to_utc(&depart_local, &option.timezone)?;
        let arrive_utc = to_utc(&arrive_local, &to.timezone)?;

        rows.push(Row {
            origin: from.locode.clone(),
            dest: to.locode.clone(),
            depart_utc: depart_utc.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            arrive_utc: arrive_utc.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            depart_local,
            arrive_local,
            fare: product.title.clone(),
            vessel: option.venue.as_ref().map(|v| v.title.clone()),
            currency: answer.currency.clone(),
            total_minor: option.pricing.total,
            best_total_minor: option.pricing_potential_best.as_ref().map(|b| b.total),
            duration_min: (arrive_utc - depart_utc).num_minutes(),
            active_period_min: minutes(&period.multi_usage_rules.active_period.period)?,
            seats_available: option.capacities.available,
            bookable: option.status == "available",
        });
    }

    Ok(serde_json::to_string(&rows)?)
}

/// A local time such as `2030-01-15T20:00:00` in an IANA zone. A time the clocks skip or repeat
/// is an error.
fn to_utc(local: &str, zone: &str) -> Result<DateTime<Utc>, Error> {
    let local: NaiveDateTime = local.parse()?;
    let zone: Tz = zone.parse()?;
    let zoned = zone.from_local_datetime(&local).single();
    Ok(zoned.ok_or("a skipped or repeated local time")?.to_utc())
}

/// An ISO 8601 duration such as `P1DT12H` in whole minutes, a day being 24 hours.
fn minutes(period: &str) -> Result<i64, Error> {
    let span: jiff::Span = period.parse()?;
    let duration = span.to_duration(SpanRelativeTo::days_are_24_hours())?;
    Ok(duration.as_secs() / 60)
}

#[derive(Serialize)]
struct Row {
    origin: String,
    dest: String,
    depart_utc: String,
    arrive_utc: String,
    depart_local: String,
    arrive_local: String,
    fare: String,
    vessel: Option<String>,
    currency: String,
    total_minor: i64,
    best_total_minor: Option<i64>,
    duration_min: i64,
    active_period_min: i64,
    seats_available: Option<i64>,
    bookable: bool,
}

#[derive(Deserialize)]
pub struct Answer {
    currency: String,
    options: Vec<TravelOption>,
    products: Vec<Product>,
    locations: Vec<Location>,
}

#[derive(Deserialize)]
struct TravelOption {
    start_date: String,
    start_time: String,
    end_date: String,
    end_time: String,
    timezone: String,
    product_id: String,
    status: String,
    venue: Option<Venue>,
    legs: Vec<Leg>,
    pricing: Pricing,
    pricing_potential_best: Option<Pricing>,
    capacities: Capacities,
}

#[derive(Deserialize)]
struct Venue {
    title: String,
}

#[derive(Deserialize)]
struct Leg {
    location: Place,
    end_location: Place,
}

#[derive(Deserialize)]
struct Place {
    id: String,
}

#[derive(Deserialize)]
struct Pricing {
    total: i64,
}

#[derive(Deserialize)]
struct Capacities {
    available: Option<i64>,
}

#[derive(Deserialize)]
struct Product {
    id: String,
    title: String,
    multi_usage_rules: UsageRules,
}

#[derive(Deserialize)]
struct UsageRules {
    active_period: ActivePeriod,
}

#[derive(Deserialize)]
struct ActivePeriod {
    #[serde(rename = "type")]
    kind: String,
    period: String,
}

#[derive(Deserialize)]
struct Location {
    id: String,
    locode: String,
    timezone: String,
}
