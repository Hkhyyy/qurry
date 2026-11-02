//! Fuzzy matching for search popups: exact > prefix > substring > letters in order. Higher scores come first; None = no match.

/// Where the GUI scores differently from the TUI and the MCP server (D2; the fuzzyScore rows of crates/shared/tests/DIVERGENCES.md)
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// The GUI's JetBrains-style scoring (Search Everywhere): a substring or letter at a word start (after _ . - or a space,
    /// or an uppercase letter) scores higher, adjacent letters earn a bonus, positions and lengths count UTF-16 code units
    /// and the length penalty is fractional. Otherwise the TUI's scoring: no word starts, UTF-8 byte positions, whole numbers
    pub word_starts: bool,
}

pub fn score(query: &str, text: &str, opts: Options) -> Option<f64> {
    if opts.word_starts { word_starts(query, text) } else { plain(query, text).map(|s| s as f64) }
}

fn plain(q: &str, text: &str) -> Option<i64> {
    if q.is_empty() {
        return Some(0);
    }
    let (q, t) = (q.to_lowercase(), text.to_lowercase());
    if t == q {
        return Some(1000);
    }
    if t.starts_with(&q) {
        return Some(800 - t.len() as i64);
    }
    if let Some(i) = t.find(&q) {
        return Some(500 - i as i64 - t.len() as i64 / 10);
    }
    let mut score = 200;
    let mut it = t.char_indices();
    let mut last = 0usize;
    for qc in q.chars().filter(|c| !c.is_whitespace()) {
        let (i, _) = it.find(|&(_, c)| c == qc)?;
        score -= i.saturating_sub(last).min(10) as i64;
        last = i + 1;
    }
    Some(score - t.len() as i64 / 10)
}

/// JavaScript's \s for one UTF-16 code unit
fn js_space(u: u16) -> bool {
    matches!(u, 0x09..=0x0d | 0x20 | 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

fn find(hay: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(hay.len()));
    }
    (from..hay.len().saturating_sub(needle.len() - 1)).find(|&i| hay[i..].starts_with(needle))
}

fn word_starts(query: &str, text: &str) -> Option<f64> {
    if query.is_empty() {
        return Some(0.0);
    }
    let q: Vec<u16> = query.to_lowercase().encode_utf16().collect();
    let t: Vec<u16> = text.to_lowercase().encode_utf16().collect();
    let orig: Vec<u16> = text.encode_utf16().collect();
    let len = t.len() as f64;
    if t == q {
        return Some(1000.0);
    }
    if t.starts_with(&q) {
        return Some(800.0 - len);
    }
    // At k: the start of a word (after _ . - or a space), or a letter the lowercasing changed (uppercase)
    let start = |k: usize| k == 0 || matches!(t[k - 1], 0x5f | 0x2e | 0x2d) || js_space(t[k - 1]) || orig.get(k) != t.get(k);
    if let Some(at) = find(&t, &q, 0) {
        return Some(if start(at) { 600.0 } else { 400.0 } - at as f64 - len * 0.1);
    }
    let mut score = 200.0;
    let (mut ti, mut prev) = (0usize, -2i64);
    for ch in query.to_lowercase().chars().filter(|&c| c != ' ') {
        let mut buf = [0u16; 2];
        let ch = ch.encode_utf16(&mut buf);
        let found = match ch {
            [u] => (ti..t.len()).find(|&k| t[k] == *u && start(k)),
            _ => None,
        };
        let found = found.or_else(|| find(&t, ch, ti))?;
        score += if found as i64 == prev + 1 { 5.0 } else { -((found - ti).min(10) as f64) };
        prev = found as i64;
        ti = found + 1;
    }
    Some(score - len * 0.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUI: Options = Options { word_starts: true };

    #[test]
    fn plain_matches_the_tui() {
        assert_eq!(score("", "users", Options::default()), Some(0.0));
        assert_eq!(score("users", "Users", Options::default()), Some(1000.0));
        assert_eq!(score("uph", "user_payment_history", Options::default()), Some(187.0));
        assert_eq!(score("su", "us", Options::default()), None);
    }

    #[test]
    fn word_starts_rank_first() {
        let s = |t: &str| score("pay", t, GUI).unwrap();
        assert!(s("user_payment") > s("repay"));
        assert!(score("ord", "CustomerOrders", GUI).unwrap() > score("ord", "customerorders", GUI).unwrap());
        assert_eq!(score("xyz", "users", GUI), None);
    }
}
