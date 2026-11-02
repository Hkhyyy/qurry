//! Display formatting shared by the result grids: fractional seconds aligned within a time column, digit grouping.

/// Columns whose fractional seconds are aligned: time types except INTERVAL
pub fn is_time_type(type_name: &str) -> bool {
    let t = type_name.to_uppercase();
    t.contains("TIME") && !t.contains("INTERVAL")
}

/// End of "hh:mm:ss" and the fractional-second digit count of a time value: `[dddd-dd-dd( |T)]hh:mm:ss[.f…]` at the start
fn time_frac(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let digits = |from: usize| b[from.min(b.len())..].iter().take_while(|c| c.is_ascii_digit()).count();
    let at = |i: usize, c: u8| b.get(i) == Some(&c);
    let two = |i: usize| b.get(i..i + 2).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let time = |i: usize| two(i) && at(i + 2, b':') && two(i + 3) && at(i + 5, b':') && two(i + 6);
    let y = digits(0);
    let dated = y >= 4 && at(y, b'-') && two(y + 1) && at(y + 3, b'-') && two(y + 4) && (at(y + 6, b' ') || at(y + 6, b'T')) && time(y + 7);
    let start = if dated { y + 7 } else if time(0) { 0 } else { return None };
    let end = start + 8;
    Some((end, if at(end, b'.') { digits(end + 1) } else { 0 }))
}

/// The longest fractional-second digit count among a column's values (0 if none)
pub fn frac_width<'a>(values: impl IntoIterator<Item = &'a str>) -> usize {
    values.into_iter().filter_map(time_frac).map(|x| x.1).max().unwrap_or(0)
}

/// 03:19:15 → 03:19:15.000000 (pads with 0 up to `width` digits; other text unchanged)
pub fn pad_frac(s: &str, width: usize) -> String {
    match time_frac(s) {
        Some((end, d)) if d < width => {
            let frac = if d > 0 { &s[end + 1..end + 1 + d] } else { "" };
            let rest = &s[end + if d > 0 { 1 + d } else { 0 }..];
            format!("{}.{frac:0<width$}{rest}", &s[..end])
        }
        _ => s.to_string(),
    }
}

/// 1234567 → "1,234,567"
pub fn thousands(n: u64) -> String {
    n.to_string().as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_values_are_padded() {
        assert_eq!(pad_frac("2026-06-06 03:19:15 +09:00", 6), "2026-06-06 03:19:15.000000 +09:00");
        assert_eq!(pad_frac("2026-06-08 10:52:05.27 +09:00", 6), "2026-06-08 10:52:05.270000 +09:00");
        assert_eq!(pad_frac("12:00:01", 3), "12:00:01.000");
        assert_eq!(pad_frac("2026-06-08T10:52:05.123456", 6), "2026-06-08T10:52:05.123456");
        assert_eq!(pad_frac("hello", 6), "hello");
        // A date part must be digits (as the GUI's pattern): no padding after other text
        assert_eq!(pad_frac("abcd-ef-gh 10:00:00", 3), "abcd-ef-gh 10:00:00");
        assert_eq!(frac_width(["03:19:15.5", "x", "09:38:06.521101"]), 6);
        assert!(is_time_type("timestamp with time zone") && !is_time_type("interval"));
    }

    #[test]
    fn thousands_groups_digits() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
