//! Architecture rule: the data source list and the passwords have one home each.
//! connections.json / connection-access.json are written only by crates/core/src/store.rs, secrets.json only by
//! crates/core/src/secrets.rs, and SecretFile is constructed only there and in lib.rs::open_state, which every app
//! (GUI, TUI, MCP) uses to open its state. Test code (`#[cfg(test)]` items, `tests/` folders) is exempt.

use std::path::{Path, PathBuf};

/// Things only the homes may contain (in code, not in comments); secret.key(.keychain) hold or point to the password key
const FORBIDDEN: &[&str] = &[
    "\"connections.json\"",
    "\"connection-access.json\"",
    "\"secrets.json\"",
    "\"secret.key\"",
    "\"secret.key.keychain\"",
    "SecretFile::",
    "SecretFile {",
];
/// Files that are a home for everything above
const HOMES: &[&str] = &["crates/core/src/store.rs", "crates/core/src/secrets.rs"];

/// Same length as `src`: comments become spaces (newlines kept); with `blank_strings`, string and char contents too
fn strip(src: &str, blank_strings: bool) -> Vec<u8> {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let blank = |out: &mut Vec<u8>, from: usize, to: usize| {
        for c in &mut out[from..to] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |p| i + p);
                blank(&mut out, i, end);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = src[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
                blank(&mut out, i, end);
                i = end;
            }
            // Raw strings (r"..", r#".."#, br#".."#): end at `"` plus the same number of `#`
            b'r' if raw_hashes(b, i).is_some() => {
                let h = raw_hashes(b, i).unwrap();
                let start = i + 2 + h;
                let close = format!("\"{}", "#".repeat(h));
                let end = src[start..].find(&close).map_or(b.len(), |p| start + p);
                if blank_strings {
                    blank(&mut out, start, end);
                }
                i = (end + close.len()).min(b.len());
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                if blank_strings {
                    blank(&mut out, i + 1, j.min(b.len()));
                }
                i = j + 1;
            }
            // Char literals ('"', '{', '\n'); a lifetime ('a) is left alone
            b'\'' if b.get(i + 1) == Some(&b'\\') => {
                let end = b[i + 2..].iter().position(|&c| c == b'\'').map_or(b.len(), |p| i + 2 + p + 1);
                if blank_strings {
                    blank(&mut out, i + 1, end - 1);
                }
                i = end;
            }
            b'\'' if b.get(i + 2) == Some(&b'\'') => {
                if blank_strings {
                    blank(&mut out, i + 1, i + 2);
                }
                i += 3;
            }
            _ => i += 1,
        }
    }
    out
}

/// The `#` count when the `r` at `i` starts a raw string (`r"`, `r#"`, `br#"`), not an identifier like `for"`
fn raw_hashes(b: &[u8], i: usize) -> Option<usize> {
    let ident = |k: usize| b[k].is_ascii_alphanumeric() || b[k] == b'_';
    let prefix_ok = i == 0 || !ident(i - 1) || (b[i - 1] == b'b' && (i == 1 || !ident(i - 2)));
    let h = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
    (prefix_ok && b.get(i + 1 + h) == Some(&b'"')).then_some(h)
}

/// End of the item starting at `from`: its matching `}`, or the `;` if that comes first
fn item_end(code: &[u8], from: usize) -> usize {
    let mut depth = 0;
    for (k, &c) in code.iter().enumerate().skip(from) {
        match c {
            b';' if depth == 0 => return k + 1,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
    }
    code.len()
}

/// Byte ranges the rule doesn't look at: `#[cfg(test)]` items, and lib.rs's `fn open_state`
fn exempt(rel: &str, code: &[u8]) -> Vec<(usize, usize)> {
    let text = String::from_utf8_lossy(code);
    let mut ranges: Vec<(usize, usize)> = text.match_indices("#[cfg(test)]").map(|(p, _)| (p, item_end(code, p + "#[cfg(test)]".len()))).collect();
    if rel == "crates/core/src/lib.rs" {
        if let Some(p) = text.find("pub fn open_state(") {
            ranges.push((p, item_end(code, p)));
        }
    }
    ranges
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name();
        if path.is_dir() {
            // Build output, frontend deps and integration tests (test code) are not scanned
            if !matches!(name.to_str(), Some("target" | "node_modules" | "tests" | "gen")) {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// Forbidden things in one file's code, as (line, pattern)
fn hits(rel: &str, src: &str) -> Vec<(usize, &'static str)> {
    let code = strip(src, false);
    let skip = exempt(rel, &strip(src, true));
    let text = String::from_utf8_lossy(&code);
    let mut found = Vec::new();
    for pat in FORBIDDEN {
        for (p, _) in text.match_indices(pat) {
            if !skip.iter().any(|&(a, b)| (a..b).contains(&p)) {
                found.push((src[..p].matches('\n').count() + 1, *pat));
            }
        }
    }
    found.sort();
    found
}

/// Every place outside the homes that writes the data source / password files or constructs SecretFile, as "file:line: what"
fn violations(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    for d in ["crates", "src-tauri", "drivers"] {
        rust_files(&root.join(d), &mut files);
    }
    // A wrong root would scan nothing and pass
    assert!(files.iter().any(|f| f.ends_with("src-tauri/src/lib.rs")), "no sources found under {}", root.display());
    let mut found = Vec::new();
    for path in files {
        let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        if HOMES.contains(&rel.as_str()) {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        found.extend(hits(&rel, &src).into_iter().map(|(line, pat)| format!("{rel}:{line}: {pat}")));
    }
    found.sort();
    found
}

#[test]
fn data_sources_and_secrets_have_one_home() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let found = violations(&root);
    assert!(
        found.is_empty(),
        "Only crates/core/src/store.rs (connections.json, connection-access.json), crates/core/src/secrets.rs (secrets.json, SecretFile) \
         and qurry_core::open_state may touch these. Open the app state with qurry_core::open_state(Some(dir)) instead:\n{}",
        found.join("\n")
    );
}

/// The scanner itself: comments, test items and open_state are skipped; code is not
#[test]
fn scanner_sees_code_only() {
    let src = r#"
// let s = SecretFile::open(&dir);
/* "secrets.json" */
fn real() { let q = '"'; let s = SecretFile::open(&dir); }
#[cfg(test)]
mod tests { fn t() { let _ = "{"; std::fs::write(dir.join("connections.json"), "x"); } }
fn after() { std::fs::write(dir.join("connections.json"), "x"); }
"#;
    let lines: Vec<usize> = hits("x.rs", src).into_iter().map(|(l, _)| l).collect();
    assert_eq!(lines, vec![4, 7]);

    // Raw strings: quotes and `//` inside them neither flip the string state nor start a comment
    let raw = "#[cfg(test)] mod t { fn f() { let j = r#\"{\"a\":1}\"#; } }\nfn after() { SecretFile::open(&d); }\n\
               fn url() { let u = br##\"\"# http://x\"##; std::fs::write(d.join(\"secret.key\"), u); }\n";
    assert_eq!(hits("x.rs", raw), vec![(2, "SecretFile::"), (3, "\"secret.key\"")]);

    let lib = "pub fn open_state(dir: P) -> R { let s = SecretFile::open(&dir)?; Ok(s) }\nfn other() { SecretFile::open(&d); }\n";
    assert_eq!(hits("crates/core/src/lib.rs", lib), vec![(2, "SecretFile::")]);
}
