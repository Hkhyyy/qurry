//! Import column mapping: which table column each file column goes into.

/// Spaces are JavaScript's \s (the GUI's /[\s_-]/): a BOM (U+FEFF) is one, NEL (U+0085) is not
fn norm(s: &str) -> String {
    let space = |c: char| u16::try_from(u32::from(c)).is_ok_and(crate::fuzzy::js_space);
    s.to_lowercase().chars().filter(|&c| !space(c) && c != '_' && c != '-').collect()
}

/// The first column with the file column's name, ignoring case, spaces, _ and -
pub fn match_column<'a>(file: &str, cols: &'a [String]) -> Option<&'a String> {
    let f = norm(file);
    cols.iter().find(|c| norm(c) == f)
}

/// Column for each file column ("" = skip). `table` None is a new table: the file's names as they are
pub fn auto_map(file: &[String], table: Option<&[String]>) -> Vec<String> {
    match table {
        None => file.to_vec(),
        Some(cols) => file.iter().map(|f| match_column(f, cols).cloned().unwrap_or_default()).collect(),
    }
}
