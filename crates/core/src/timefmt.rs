//! Dates and times as text, the same as before chrono was replaced with time (#199): years outside 0..=9999 with a sign (-0043,
//! +10000), a fraction to its last 3, 6 or 9 digits, offsets as +09:00 (+08:27:52 with seconds), and only the years chrono held
use std::ops::RangeInclusive;
use std::time::SystemTime;
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

/// The years chrono could hold: a value outside them showed as its type (<TIMESTAMP>), and still does
const YEARS: RangeInclusive<i32> = -262_143..=262_142;

pub(crate) fn in_range(d: Date) -> bool {
    YEARS.contains(&d.year())
}

/// 2026-11-09, -0043-03-15, +10000-01-01
pub(crate) fn date(d: Date) -> String {
    let y = d.year();
    let year = if (0..=9999).contains(&y) { format!("{y:04}") } else { format!("{y:+05}") };
    format!("{year}-{:02}-{:02}", u8::from(d.month()), d.day())
}

/// 13:14:15 (no fraction)
pub(crate) fn hms(t: Time) -> String {
    format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
}

/// 13:14:15, 13:14:15.500, 13:14:15.000001, 13:14:15.123456789
pub(crate) fn time(t: Time) -> String {
    let n = t.nanosecond();
    let frac = if n == 0 {
        String::new()
    } else if n.is_multiple_of(1_000_000) {
        format!(".{:03}", n / 1_000_000)
    } else if n.is_multiple_of(1_000) {
        format!(".{:06}", n / 1_000)
    } else {
        format!(".{n:09}")
    };
    hms(t) + &frac
}

/// 2026-11-09 13:14:15.500
pub(crate) fn datetime(t: PrimitiveDateTime) -> String {
    format!("{} {}", date(t.date()), time(t.time()))
}

/// +09:00, -03:30, +08:27:52
pub(crate) fn offset(o: UtcOffset) -> String {
    let s = o.whole_seconds();
    let (sign, s) = if s < 0 { ('-', -s) } else { ('+', s) };
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if s == 0 { format!("{sign}{h:02}:{m:02}") } else { format!("{sign}{h:02}:{m:02}:{s:02}") }
}

/// `t` in this computer's time zone, as chrono's Local had it: TZ when set (a name or a POSIX rule), otherwise /etc/localtime;
/// UTC without either
pub(crate) fn local(t: SystemTime) -> OffsetDateTime {
    let utc = OffsetDateTime::from(t);
    let zone = std::env::var("TZ").ok().and_then(|v| tz::TimeZone::from_posix_tz(&v).ok()).or_else(|| tz::TimeZone::local().ok());
    let off = zone.and_then(|z| z.find_local_time_type(utc.unix_timestamp()).ok().map(|l| l.ut_offset())).unwrap_or(0);
    UtcOffset::from_whole_seconds(off).map_or(utc, |o| utc.to_offset(o))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    #[test]
    fn text() {
        let d = |y, m, d| Date::from_calendar_date(y, m, d).unwrap();
        assert_eq!(date(d(2026, Month::November, 9)), "2026-11-09");
        assert_eq!(date(d(-43, Month::March, 15)), "-0043-03-15");
        assert_eq!(date(d(10000, Month::January, 1)), "+10000-01-01");
        let t = |n| Time::from_hms_nano(13, 14, 15, n).unwrap();
        assert_eq!([0, 500_000_000, 1_000, 10_000, 123_456_789].map(|n| time(t(n))), ["13:14:15", "13:14:15.500", "13:14:15.000001", "13:14:15.000010", "13:14:15.123456789"]);
        let o = |s| offset(UtcOffset::from_whole_seconds(s).unwrap());
        assert_eq!([0, 9 * 3600, -(3 * 3600 + 30 * 60), 8 * 3600 + 27 * 60 + 52, -1].map(o), ["+00:00", "+09:00", "-03:30", "+08:27:52", "-00:00:01"]);
    }
}
