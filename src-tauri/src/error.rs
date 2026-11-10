use serde::{Serialize, Serializer};

/// Application error. Serialized to the frontend as `{ kind, message }`.
/// Messages must never contain secret material.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    #[error("Not found: {0}")]
    NotFound(String),
    /// Target already exists; the UI asks before overwriting.
    #[error("{0}")]
    Exists(String),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Credential store error: {0}")]
    Credentials(String),
    #[error("{0}")]
    Ssh(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation",
            AppError::NotFound(_) => "notFound",
            AppError::Exists(_) => "exists",
            AppError::Database(_) => "database",
            AppError::Credentials(_) => "credentials",
            AppError::Ssh(_) => "ssh",
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::Internal(_) => "internal",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
