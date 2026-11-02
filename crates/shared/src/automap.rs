//! Import column mapping: which table column each file column goes into.

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '-').collect()
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
