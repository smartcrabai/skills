use thiserror::Error;

/// Crate-wide error type.
#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("git clone failed: {0}")]
    GitClone(String),

    #[error("not implemented")]
    NotImplemented,

    #[error("skill not found: {0}")]
    SkillNotFound(String),

    #[error("skill already installed: {0}")]
    DuplicateSkill(String),

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("invalid source: {0}")]
    InvalidSource(String),

    #[error("invalid scope: {0}")]
    InvalidScope(String),

    #[error("a TTY is required for this prompt")]
    TtyRequired,

    #[error("SKILL.md not found in {0}")]
    SkillMdMissing(String),
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, Error>;
