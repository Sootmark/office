//! W3C date and time formats (W3CDTF, a profile of ISO 8601), as
//! `dcterms:created` and `dcterms:modified` are written: `2012-11-07`,
//! `2012-11-07T23:29Z`, `2012-11-07T23:29:00Z`,
//! `2012-03-05T20:40:00.1234567+01:00`. A year or a year and month alone
//! are read as their first day. A time without a zone is a wall-clock time
//! in an unknown zone.

use common::time::{
    civil_from_days, days_from_civil, Precision, Ts, TICKS_PER_DAY, TICKS_PER_SECOND,
};

/// Fraction digits a tick holds (100 ns).
const TICK_DIGITS: usize = 7;
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3600;

/// The time `text` gives, `None` when it isn't W3CDTF.
pub fn parse(text: &str) -> Option<Ts> {
    let text = text.trim();
    let (date, time) = match text.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let days = date_days(date)?;
    let Some(time) = time else {
        return days
            .checked_mul(TICKS_PER_DAY)
            .map(|t| Ts::from_ticks(t, Precision::Day));
    };
    let (clock, offset_seconds) = split_zone(time)?;
    let (seconds_of_day, fraction, precision) = clock_time(clock)?;
    let ticks = days.checked_mul(TICKS_PER_DAY)?.checked_add(
        (seconds_of_day - offset_seconds.unwrap_or(0)) * TICKS_PER_SECOND + fraction,
    )?;
    Some(match offset_seconds {
        Some(_) => Ts::from_ticks(ticks, precision),
        None => Ts::from_local_ticks(ticks, precision),
    })
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD` as days since 1970-01-01.
fn date_days(date: &str) -> Option<i64> {
    let mut fields = date.split('-');
    let year = number(fields.next()?, 4)?;
    let month = fields.next().map_or(Some(1), |m| number(m, 2))?;
    let day = fields.next().map_or(Some(1), |d| number(d, 2))?;
    if fields.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let (month, day) = (u32::try_from(month).ok()?, u32::try_from(day).ok()?);
    let days = days_from_civil(year, month, day);
    (civil_from_days(days) == (year, month, day)).then_some(days)
}

/// The clock and the zone's offset east of UTC in seconds (`None` when no
/// zone is given).
fn split_zone(time: &str) -> Option<(&str, Option<i64>)> {
    if let Some(clock) = time.strip_suffix('Z') {
        return Some((clock, Some(0)));
    }
    let Some(at) = time.rfind(['+', '-']) else {
        return Some((time, None));
    };
    let (clock, zone) = time.split_at(at);
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let (hours, minutes) = zone.get(1..)?.split_once(':')?;
    let (hours, minutes) = (number(hours, 2)?, number(minutes, 2)?);
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some((
        clock,
        Some(sign * (hours * SECONDS_PER_HOUR + minutes * SECONDS_PER_MINUTE)),
    ))
}

/// `hh:mm`, `hh:mm:ss` or `hh:mm:ss.f…`: seconds of the day, the fraction
/// in ticks (digits past the seventh dropped), and the precision.
fn clock_time(clock: &str) -> Option<(i64, i64, Precision)> {
    let (whole, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut fields = whole.split(':');
    let hours = number(fields.next()?, 2)?;
    let minutes = number(fields.next()?, 2)?;
    let seconds = fields.next().map_or(Some(0), |s| number(s, 2))?;
    let digits_ok = fraction.bytes().all(|b| b.is_ascii_digit());
    let fraction_ok = digits_ok && !(fraction.is_empty() && clock.contains('.'));
    if fields.next().is_some() || hours > 23 || minutes > 59 || seconds > 59 || !fraction_ok {
        return None;
    }
    let kept = fraction
        .get(..fraction.len().min(TICK_DIGITS))
        .unwrap_or_default();
    let ticks = if kept.is_empty() {
        0
    } else {
        format!("{kept:0<TICK_DIGITS$}").parse().ok()?
    };
    let precision = match kept.len() {
        0 => Precision::Second,
        1..=3 => Precision::Millisecond,
        4..=6 => Precision::Microsecond,
        _ => Precision::Tick,
    };
    Some((
        hours * SECONDS_PER_HOUR + minutes * SECONDS_PER_MINUTE + seconds,
        ticks,
        precision,
    ))
}

/// Exactly `digits` ASCII digits.
fn number(text: &str, digits: usize) -> Option<i64> {
    (text.len() == digits && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::time::Semantic;

    fn iso(text: &str) -> Option<String> {
        parse(text).and_then(|t| t.to_iso8601())
    }

    #[test]
    fn office_forms() {
        assert_eq!(
            iso("2012-11-07T23:29:00Z").unwrap(),
            "2012-11-07T23:29:00.0000000Z"
        );
        assert_eq!(
            iso("2012-03-05T20:40:00.1234567Z").unwrap(),
            "2012-03-05T20:40:00.1234567Z"
        );
        assert_eq!(
            iso("2012-03-05T20:40:00.123456789Z").unwrap(),
            "2012-03-05T20:40:00.1234567Z"
        );
        assert_eq!(
            iso("2012-03-05T20:40Z").unwrap(),
            "2012-03-05T20:40:00.0000000Z"
        );
    }

    #[test]
    fn zones_and_dates() {
        assert_eq!(
            iso("2012-03-05T00:30:00+01:00").unwrap(),
            "2012-03-04T23:30:00.0000000Z"
        );
        assert_eq!(
            iso("2012-03-05T23:30:00-02:30").unwrap(),
            "2012-03-06T02:00:00.0000000Z"
        );
        assert_eq!(iso("2012-03").unwrap(), "2012-03-01T00:00:00.0000000Z");
        assert_eq!(parse("2012").unwrap().precision(), Precision::Day);
        let local = parse("2012-03-05T20:40:00").unwrap();
        assert_eq!(local.semantic(), Semantic::LocalUnknownZone);
        assert_eq!(
            parse("2012-03-05T20:40:00.5Z").unwrap().precision(),
            Precision::Millisecond
        );
    }

    #[test]
    fn not_dates() {
        for text in [
            "",
            "2012-02-30",
            "2012-13-01",
            "2012-11-07T24:00Z",
            "12-11-07",
            "2012-11-07T23:29:00.Z",
            "2012-11-07T23:29:00+1:00",
            "x",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }
}
