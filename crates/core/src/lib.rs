//! OpenPDM core: git repository access, a content-addressed SQLite index,
//! and pluggable CAD file handlers.
//!
//! Design notes:
//! * Git is the source of truth; the SQLite [`Index`] is a rebuildable cache.
//! * The index is *content-addressed* (keyed by git blob oid) so identical
//!   revisions across revisions and repositories dedupe naturally, and a future
//!   shared "vault" repository is a data-model change, not a re-architecture.
//! * File-format specific behaviour lives behind the [`handlers::CadHandler`]
//!   trait. The core stays format-agnostic.

pub mod convert;
pub mod error;
pub mod git;
pub mod handlers;
pub mod index;
pub mod mesh;

pub use convert::{
    CachedConverter, CommandConverter, Converter, ConverterRegistry, NullConverter, TARGET_STL,
};
pub use error::{Error, Result};
pub use git::{FileEntry, Project, Revision, WORKTREE};
pub use handlers::{
    is_project_file, CadHandler, FileMetadata, HandlerRegistry, ShapePayload, PROJECT_EXTENSIONS,
};
pub use index::{Index, IndexStats};
pub use mesh::Mesh;
