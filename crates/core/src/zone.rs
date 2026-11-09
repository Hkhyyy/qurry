//! A session's time zone, for showing PostgreSQL `timestamptz` as the server shows it (psql): in the session's TimeZone, with
//! that moment's offset. A zone name's rules (DST, past offsets) come from this computer's tz database (/usr/share/zoneinfo, on
//! macOS and Linux), so no tz data is built in. A setting it lacks (`+09:00`, `<+09>-09`, or a computer without the database)
//! takes the offset the server says the session has now, so the text is still the right moment
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const ZONEINFO: &str = "/usr/share/zoneinfo";

#[derive(Clone, Debug)]
pub enum Zone {
    Rules(Arc<tz::TimeZone>),
    /// Seconds east of UTC
    Fixed(i32),
}

impl Zone {
    pub const UTC: Zone = Zone::Fixed(0);

    /// `name`: the session's TimeZone setting; `now_offset`: its offset from UTC now in seconds, as the server says
    pub fn new(name: &str, now_offset: i32) -> Zone {
        rules(name).map(Zone::Rules).unwrap_or(Zone::Fixed(now_offset))
    }

    fn offset_at(&self, unix_secs: i64) -> i32 {
        match self {
            Zone::Fixed(o) => *o,
            Zone::Rules(z) => z.find_local_time_type(unix_secs).map(|t| t.ut_offset()).unwrap_or(0),
        }
    }

    /// A UTC time as `2026-11-09 13:00:00 +09:00` (fractions as stored; an offset with seconds, as before 1900, as `+08:27:52`)
    pub fn format(&self, utc: NaiveDateTime) -> Option<String> {
        let off = self.offset_at(utc.and_utc().timestamp());
        // None out of chrono's range once the offset is added (the first or last hours it can hold)
        utc.checked_add_signed(chrono::TimeDelta::seconds(off.into()))?;
        let t = DateTime::<FixedOffset>::from_naive_utc_and_offset(utc, FixedOffset::east_opt(off)?);
        Some(t.format(if off % 60 == 0 { "%Y-%m-%d %H:%M:%S%.f %:z" } else { "%Y-%m-%d %H:%M:%S%.f %::z" }).to_string())
    }
}

/// The tz database's rules for `name`, read once per name. Only a zone name (`Asia/Seoul`, `UTC`, `EST5EDT`) is looked up, never a
/// path out of the database's folder
fn rules(name: &str) -> Option<Arc<tz::TimeZone>> {
    let plain = name.len() <= 64
        && name.split('/').all(|p| !p.is_empty() && !p.starts_with('.') && p.chars().all(|c| c.is_ascii_alphanumeric() || "_-+".contains(c)));
    if !plain {
        return None;
    }
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<tz::TimeZone>>>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(name.to_string())
        .or_insert_with(|| {
            let path = std::path::Path::new(ZONEINFO).join(name);
            // A TZif file is a few KB: anything else there (a folder, a large file) is not one
            std::fs::metadata(&path).ok().filter(|m| m.is_file() && m.len() < 1 << 20)?;
            tz::TimeZone::from_tz_data(&std::fs::read(&path).ok()?).ok().map(Arc::new)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    #[test]
    fn fixed_offsets_need_no_tz_data() {
        let t = utc(2026, 11, 9, 4, 0);
        assert_eq!(Zone::UTC.format(t).as_deref(), Some("2026-11-09 04:00:00 +00:00"));
        assert_eq!(Zone::Fixed(9 * 3600).format(t).as_deref(), Some("2026-11-09 13:00:00 +09:00"));
        assert_eq!(Zone::Fixed(-(5 * 3600 + 30 * 60)).format(t).as_deref(), Some("2026-11-08 22:30:00 -05:30"));
        // A setting the tz database doesn't name takes the server's offset: PostgreSQL reads '+09:00' as POSIX (9 hours west)
        assert_eq!(Zone::new("+09:00", -9 * 3600).format(t).as_deref(), Some("2026-11-08 19:00:00 -09:00"));
        assert_eq!(Zone::new("<+09>-09", 9 * 3600).format(t).as_deref(), Some("2026-11-09 13:00:00 +09:00"));
        let frac = t + chrono::TimeDelta::microseconds(1500);
        assert_eq!(Zone::UTC.format(frac).as_deref(), Some("2026-11-09 04:00:00.001500 +00:00"));
    }

    #[test]
    fn names_never_leave_the_database_folder() {
        for bad in ["../../etc/passwd", "/etc/passwd", "Asia/../../etc/hosts", ".hidden", "", "a//b", "Asia/Seoul\0", "Asia Seoul"] {
            assert!(rules(bad).is_none(), "{bad:?}");
        }
        assert!(rules("zone.tab").is_none(), "a file there that isn't a zone");
    }

    /// The zone's own offset at each moment, across a DST change (needs /usr/share/zoneinfo, as on macOS and Linux)
    #[test]
    fn named_zones_follow_their_rules() {
        if !std::path::Path::new(ZONEINFO).join("Europe/Berlin").exists() {
            return;
        }
        let berlin = Zone::new("Europe/Berlin", 0);
        assert!(matches!(berlin, Zone::Rules(_)));
        // Summer time ends 2026-10-25 01:00 UTC
        assert_eq!(berlin.format(utc(2026, 10, 25, 0, 30)).as_deref(), Some("2026-10-25 02:30:00 +02:00"));
        assert_eq!(berlin.format(utc(2026, 10, 25, 1, 30)).as_deref(), Some("2026-10-25 02:30:00 +01:00"));
        // Past the last transition in the file: the rule at its end (POSIX TZ footer)
        assert_eq!(berlin.format(utc(2100, 7, 1, 0, 0)).as_deref(), Some("2100-07-01 02:00:00 +02:00"));
        let seoul = Zone::new("Asia/Seoul", 0);
        assert_eq!(seoul.format(utc(2026, 11, 9, 4, 0)).as_deref(), Some("2026-11-09 13:00:00 +09:00"));
        // Korea had summer time in 1988, and local mean time before 1908
        assert_eq!(seoul.format(utc(1988, 7, 1, 0, 0)).as_deref(), Some("1988-07-01 10:00:00 +10:00"));
        assert_eq!(seoul.format(utc(1900, 1, 1, 0, 0)).as_deref(), Some("1900-01-01 08:27:52 +08:27:52"));
        assert_eq!(Zone::new("UTC", 3600).format(utc(2026, 1, 1, 0, 0)).as_deref(), Some("2026-01-01 00:00:00 +00:00"), "the database wins");
    }
}
