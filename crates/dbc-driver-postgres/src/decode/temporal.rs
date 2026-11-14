//! Dates and times. PostgreSQL counts from 2000-01-01 in days or microseconds.

use dbc_core::Value;

use super::reader::Reader;

const MICROS_PER_SECOND: i64 = 1_000_000;
const SECONDS_PER_DAY: i64 = 86_400;
const MICROS_PER_DAY: i64 = MICROS_PER_SECOND * SECONDS_PER_DAY;
/// Days from 1970-01-01 to 2000-01-01.
const PG_EPOCH_DAYS: i64 = 10_957;
const MONTHS_PER_YEAR: i32 = 12;

pub(super) fn decode(name: &str, r: &mut Reader<'_>) -> Option<Value> {
    Some(match name {
        "date" => Value::Date(date(r.i32()?)),
        "time" => Value::Time(time_of_day(r.i64()?)),
        "timetz" => {
            let micros = r.i64()?;
            // Stored as seconds *west* of UTC.
            let offset = -i64::from(r.i32()?);
            Value::Time(format!("{}{}", time_of_day(micros), offset_text(offset)))
        }
        "timestamp" => Value::Timestamp(timestamp(r.i64()?, false)),
        "timestamptz" => Value::TimestampTz(timestamp(r.i64()?, true)),
        "interval" => {
            let micros = r.i64()?;
            let days = r.i32()?;
            let months = r.i32()?;
            Value::Interval(interval(months, days, micros))
        }
        _ => return None,
    })
}

fn date(days: i32) -> String {
    match days {
        i32::MAX => "infinity".to_owned(),
        i32::MIN => "-infinity".to_owned(),
        _ => civil(i64::from(days) + PG_EPOCH_DAYS),
    }
}

/// `timestamptz` arrives in UTC and is shown with a `+00:00` offset.
fn timestamp(micros: i64, with_zone: bool) -> String {
    match micros {
        i64::MAX => return "infinity".to_owned(),
        i64::MIN => return "-infinity".to_owned(),
        _ => {}
    }
    let days = micros.div_euclid(MICROS_PER_DAY);
    let within = micros.rem_euclid(MICROS_PER_DAY);
    let zone = if with_zone { "+00:00" } else { "" };
    format!(
        "{}T{}{zone}",
        civil(days + PG_EPOCH_DAYS),
        time_of_day(within)
    )
}

/// `HH:MM:SS` plus fractional seconds without trailing zeros.
fn time_of_day(micros: i64) -> String {
    let seconds = micros.div_euclid(MICROS_PER_SECOND);
    let fraction = micros.rem_euclid(MICROS_PER_SECOND);
    let text = format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    );
    if fraction == 0 {
        text
    } else {
        let digits = format!("{fraction:06}");
        format!("{text}.{}", digits.trim_end_matches('0'))
    }
}

fn offset_text(seconds: i64) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let abs = seconds.abs();
    format!("{sign}{:02}:{:02}", abs / 3600, abs / 60 % 60)
}

/// PostgreSQL's default `postgres` interval style: `1 year 2 mons 3 days 04:05:06`.
/// As in PostgreSQL, a unit is singular only for exactly 1, and once a component is negative,
/// later non-negative components carry an explicit `+`.
fn interval(months: i32, days: i32, micros: i64) -> String {
    let mut parts = Vec::new();
    let mut seen_negative = false;
    let years = months / MONTHS_PER_YEAR;
    let fields = [
        (years, "year"),
        (months % MONTHS_PER_YEAR, "mon"),
        (days, "day"),
    ];
    for (value, unit) in fields {
        if value == 0 {
            continue;
        }
        let plus = if seen_negative && value > 0 { "+" } else { "" };
        let plural = if value == 1 { "" } else { "s" };
        parts.push(format!("{plus}{value} {unit}{plural}"));
        seen_negative |= value < 0;
    }
    if micros != 0 || parts.is_empty() {
        let sign = if micros < 0 {
            "-"
        } else if seen_negative {
            "+"
        } else {
            ""
        };
        parts.push(format!("{sign}{}", time_of_day(micros.abs())));
    }
    parts.join(" ")
}

/// `YYYY-MM-DD` for days since 1970-01-01 (proleptic Gregorian; Howard Hinnant's algorithm).
fn civil(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    if year < 0 {
        format!("-{:04}-{month:02}-{day:02}", -year)
    } else {
        format!("{year:04}-{month:02}-{day:02}")
    }
}
