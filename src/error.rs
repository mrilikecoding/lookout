use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("lookout overloaded: state queue is full")]
    Overloaded,

    #[error("invalid argument: {0}")]
    InvalidArg(String),

    #[error("image path {0:?} is outside the allowlist")]
    PathNotAllowed(std::path::PathBuf),

    #[error("image decode failed: {0}")]
    ImageDecode(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("internal: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages_are_useful() {
        let e = Error::InvalidArg("rows missing".into());
        assert_eq!(e.to_string(), "invalid argument: rows missing");
    }

    #[test]
    fn path_not_allowed_includes_path() {
        let e = Error::PathNotAllowed(std::path::PathBuf::from("/etc/passwd"));
        assert!(e.to_string().contains("/etc/passwd"));
    }
}
