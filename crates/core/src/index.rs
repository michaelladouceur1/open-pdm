use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{Project, Result};

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- A registered project (a git working tree or bare repo).
CREATE TABLE IF NOT EXISTS repository (
    id         INTEGER PRIMARY KEY,
    path       TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);

-- Content-addressed blob identity. Keyed by git blob oid, so identical
-- content is stored once regardless of how many revisions or repositories
-- reference it. This is what keeps a future shared "vault" repo cheap.
CREATE TABLE IF NOT EXISTS blob (
    oid       TEXT PRIMARY KEY,
    size      INTEGER NOT NULL,
    is_binary INTEGER NOT NULL,
    ext       TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS revision (
    repo_id  INTEGER NOT NULL,
    oid      TEXT NOT NULL,
    short    TEXT NOT NULL,
    summary  TEXT NOT NULL,
    author   TEXT NOT NULL,
    email    TEXT NOT NULL,
    time     INTEGER NOT NULL,
    parents  INTEGER NOT NULL,
    PRIMARY KEY (repo_id, oid)
);

-- The mapping from (revision, path) to content. The join through blob.oid is
-- the content-addressed edge that lets the same part be recognised across
-- revisions and repositories.
CREATE TABLE IF NOT EXISTS file_revision (
    repo_id    INTEGER NOT NULL,
    commit_oid TEXT NOT NULL,
    path       TEXT NOT NULL,
    blob_oid   TEXT NOT NULL,
    PRIMARY KEY (repo_id, commit_oid, path)
);

CREATE INDEX IF NOT EXISTS idx_file_revision_blob ON file_revision(blob_oid);
CREATE INDEX IF NOT EXISTS idx_revision_time ON revision(repo_id, time DESC);
CREATE INDEX IF NOT EXISTS idx_blob_ext ON blob(ext);
"#;

/// Summary of what an indexing run (or the current index) contains.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexStats {
    pub repositories: u64,
    pub revisions: u64,
    pub file_revisions: u64,
    /// Distinct blobs (content addresses) known to the index.
    pub unique_blobs: u64,
    /// Sum of blob sizes counted once per distinct blob.
    pub unique_bytes: u64,
    /// Sum of blob sizes counted once per (revision, path) reference.
    pub referenced_bytes: u64,
    /// Bytes that content-addressing saved by not storing duplicates.
    pub dedup_saved_bytes: u64,
}

/// The SQLite index. Per-user and local; never committed to a repository.
pub struct Index {
    conn: Connection,
}

impl Index {
    /// Open (creating if needed) an index database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        Self::from_connection(conn)
    }

    /// An ephemeral in-memory index (handy for tests and one-shot queries).
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO NOTHING",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get(0))
            .optional()?;
        Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0))
    }

    /// Register a project path, returning its stable repository id.
    pub fn register_repository(&self, path: &str) -> Result<i64> {
        let now = now_unix();
        self.conn.execute(
            "INSERT INTO repository(path, created_at) VALUES(?1, ?2)
             ON CONFLICT(path) DO NOTHING",
            params![path, now],
        )?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM repository WHERE path = ?1",
            params![path],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// Index the most recent `limit` revisions of `project`. Idempotent:
    /// blob/revision rows use upserts and are safe to re-run. Designed to be
    /// fully rebuildable from git.
    pub fn index(&mut self, project: &Project, limit: usize) -> Result<IndexStats> {
        self.index_filtered(project, limit, |_| true)
    }

    /// Like [`Self::index`], but only records files for which `tracked` is
    /// true. Used to honour the app- and project-level tracking settings, which
    /// exclude user-deselected file types from the index.
    pub fn index_filtered(
        &mut self,
        project: &Project,
        limit: usize,
        tracked: impl Fn(&str) -> bool,
    ) -> Result<IndexStats> {
        let repo_path = project.root().to_string_lossy().to_string();
        let repo_id = self.register_repository(&repo_path)?;

        let revisions = project.revisions(limit)?;
        let tx = self.conn.transaction()?;

        // Drop rows for file types that are no longer tracked, so narrowing the
        // settings takes effect when the index is rebuilt (inserts below only
        // add, they never remove).
        let stale: Vec<(String, String)> = {
            let mut stmt =
                tx.prepare("SELECT commit_oid, path FROM file_revision WHERE repo_id = ?1")?;
            let rows = stmt.query_map(params![repo_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.filter_map(|row| row.ok())
                .filter(|(_, path)| !tracked(path))
                .collect()
        };
        for (commit_oid, path) in stale {
            tx.execute(
                "DELETE FROM file_revision WHERE repo_id = ?1 AND commit_oid = ?2 AND path = ?3",
                params![repo_id, commit_oid, path],
            )?;
        }

        for revision in &revisions {
            tx.execute(
                "INSERT INTO revision(repo_id, oid, short, summary, author, email, time, parents)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(repo_id, oid) DO UPDATE SET
                     short=excluded.short, summary=excluded.summary, author=excluded.author,
                     email=excluded.email, time=excluded.time, parents=excluded.parents",
                params![
                    repo_id,
                    revision.oid,
                    revision.short,
                    revision.summary,
                    revision.author,
                    revision.email,
                    revision.time,
                    revision.parents as i64,
                ],
            )?;

            for file in project.files_at(Some(&revision.oid))? {
                if !tracked(&file.path) {
                    continue;
                }
                tx.execute(
                    "INSERT INTO blob(oid, size, is_binary, ext) VALUES(?1, ?2, ?3, ?4)
                     ON CONFLICT(oid) DO NOTHING",
                    params![
                        file.oid,
                        file.size as i64,
                        file.is_binary as i64,
                        extension_of(&file.path),
                    ],
                )?;
                tx.execute(
                    "INSERT INTO file_revision(repo_id, commit_oid, path, blob_oid)
                     VALUES(?1, ?2, ?3, ?4)
                     ON CONFLICT(repo_id, commit_oid, path) DO UPDATE SET
                         blob_oid=excluded.blob_oid",
                    params![repo_id, revision.oid, file.path, file.oid],
                )?;
            }
        }

        tx.commit()?;
        self.stats_for(repo_id)
    }

    /// Aggregate stats across the whole index.
    pub fn stats(&self) -> Result<IndexStats> {
        self.stats_filter(None)
    }

    /// Aggregate stats for a single registered repository.
    pub fn stats_for(&self, repo_id: i64) -> Result<IndexStats> {
        self.stats_filter(Some(repo_id))
    }

    fn stats_filter(&self, repo_id: Option<i64>) -> Result<IndexStats> {
        let repositories: u64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM repository", [], |r| r.get(0))?;

        let (rev_where, fr_where, rev_param) = match repo_id {
            Some(id) => (" WHERE repo_id = ?1", " WHERE fr.repo_id = ?1", vec![id]),
            None => ("", "", vec![]),
        };
        let revisions: u64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM revision{rev_where}"),
            rusqlite::params_from_iter(rev_param.iter()),
            |r| r.get(0),
        )?;

        let file_revisions: u64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM file_revision{rev_where}"),
            rusqlite::params_from_iter(rev_param.iter()),
            |r| r.get(0),
        )?;

        // Blob stats are global (content is shared across repositories).
        let (unique_blobs, unique_bytes): (u64, u64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size), 0) FROM blob",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        let referenced_bytes: u64 = self.conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(b.size), 0)
                 FROM file_revision fr JOIN blob b ON b.oid = fr.blob_oid{fr_where}"
            ),
            rusqlite::params_from_iter(rev_param.iter()),
            |r| r.get(0),
        )?;

        Ok(IndexStats {
            repositories,
            revisions,
            file_revisions,
            unique_blobs,
            unique_bytes,
            referenced_bytes,
            dedup_saved_bytes: referenced_bytes.saturating_sub(unique_bytes),
        })
    }

    /// Distinct content addresses, useful for a global "parts" view.
    pub fn distinct_blobs(&self) -> Result<Vec<(String, u64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT oid, size, ext FROM blob ORDER BY size DESC")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

fn extension_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(repo: &git2::Repository, name: &str, contents: &[u8], msg: &str) {
        std::fs::write(repo.workdir().unwrap().join(name), contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = head.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents)
            .unwrap();
    }

    #[test]
    fn indexes_and_dedupes_content() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "Test").unwrap();
            cfg.set_str("user.email", "test@example.com").unwrap();
        }
        // Two revisions, same file content -> one distinct blob.
        commit(&repo, "part.stl", b"identical bytes", "r1");
        commit(&repo, "other.txt", b"something else", "r2");

        let project = Project::open(dir.path()).unwrap();
        let mut index = Index::in_memory().unwrap();
        let stats = index.index(&project, 100).unwrap();

        assert_eq!(stats.revisions, 2);
        // r1: part.stl ; r2: part.stl + other.txt = 3 file-revision rows.
        assert_eq!(stats.file_revisions, 3);
        assert_eq!(stats.unique_blobs, 2);
        assert!(stats.dedup_saved_bytes > 0);
        assert!(stats.referenced_bytes > stats.unique_bytes);
        assert_eq!(index.schema_version().unwrap(), 1);
    }

    #[test]
    fn index_filter_excludes_and_prunes_untracked_files() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "Test").unwrap();
            cfg.set_str("user.email", "test@example.com").unwrap();
        }
        commit(&repo, "part.stl", b"mesh", "r1");
        commit(&repo, "scene.glb", b"scene", "r2");

        let project = Project::open(dir.path()).unwrap();
        let mut index = Index::in_memory().unwrap();
        // Index everything: part.stl at r1+r2 and scene.glb at r2.
        assert_eq!(index.index(&project, 100).unwrap().file_revisions, 3);

        // Re-index ignoring .glb: its previously indexed row is pruned.
        let filtered = index
            .index_filtered(&project, 100, |path| !path.ends_with(".glb"))
            .unwrap();
        assert_eq!(filtered.file_revisions, 2);

        // The blob itself is shared content and is intentionally kept.
        assert_eq!(index.distinct_blobs().unwrap().len(), 2);
    }
}
