use serde::{Serialize, Serializer, ser::SerializeStruct};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The unlock window is closed. The frontend prompts Touch ID and retries.
    #[error("Vault is locked")]
    Locked,
    #[error("Not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Auth(String),
    #[error("{0}")]
    Crypto(String),
    #[error("{0}")]
    Keychain(String),
    #[error("Database error: {0}")]
    Database(rusqlite::Error),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::NotFound => "notFound",
            Self::Invalid(_) => "invalid",
            Self::Auth(_) => "auth",
            Self::Crypto(_) => "crypto",
            Self::Keychain(_) => "keychain",
            Self::Database(_) => "database",
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => Self::NotFound,
            e => Self::Database(e),
        }
    }
}

/// Serialized as `{ kind, message }` so the frontend can branch on `kind`.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}
