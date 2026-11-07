use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{}", db_text(.0))]
    Db(#[from] sqlx::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Config file error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// The driver's words, with PostgreSQL's position in the statement when the server gave one (the console marks that spot:
/// qurry_shared::errors::error_position)
fn db_text(e: &sqlx::Error) -> String {
    use sqlx::postgres::{PgDatabaseError, PgErrorPosition};
    let pos = match e {
        sqlx::Error::Database(d) => d.try_downcast_ref::<PgDatabaseError>().and_then(|p| match p.position() {
            Some(PgErrorPosition::Original(n)) => Some(n),
            _ => None,
        }),
        _ => None,
    };
    match pos {
        Some(n) => qurry_shared::errors::with_position(&e.to_string(), n),
        None => e.to_string(),
    }
}
