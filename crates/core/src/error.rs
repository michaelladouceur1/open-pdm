use thiserror::Error;

/// Errors surfaced by the core crate.
#[derive(Debug, Error)]
pub enum Error {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not a git repository: {0}")]
    NotARepository(String),

    #[error("revision not found: {0}")]
    UnknownRevision(String),

    #[error("operation requires a working tree: {0}")]
    NoWorktree(String),

    #[error("no handler registered for: {0}")]
    Unsupported(String),

    #[error("nothing staged to commit")]
    NothingStaged,

    #[error("commit message is required")]
    EmptyMessage,

    #[error("no remote repository configured — add one in Project settings")]
    NoRemoteConfigured,

    #[error("repository has no commits to push")]
    NoCommits,

    #[error("branch not found: {0}")]
    UnknownBranch(String),

    #[error("invalid remote repository URL: {0}")]
    InvalidRemote(String),

    #[error("git push failed: {0}")]
    RemoteFailed(String),

    #[error("failed to parse {format}: {message}")]
    Parse { format: String, message: String },

    #[error("conversion failed: {0}")]
    Conversion(String),
}

impl Error {
    /// Convenience constructor for parse errors.
    pub fn parse(format: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Parse {
            format: format.into(),
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
