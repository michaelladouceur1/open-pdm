use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// A single commit, flattened for transport to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Revision {
    pub oid: String,
    pub short: String,
    pub summary: String,
    pub author: String,
    pub email: String,
    /// Seconds since the Unix epoch (committer time).
    pub time: i64,
    pub parents: usize,
}

/// A file present in a revision's tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    /// Git blob object id (content address).
    pub oid: String,
    pub size: u64,
    pub is_binary: bool,
}

/// A thin, git-backed view of a project.
///
/// Git is the source of truth; this type only reads. Writes, network and
/// LFS/lock operations are intentionally left to the `git`/`git-lfs`
/// subprocess layer (not part of this first pass).
pub struct Project {
    repo: git2::Repository,
    root: PathBuf,
}

impl Project {
    /// Discover the repository containing `path` (walks up to the repo root).
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let display = path.as_ref().display().to_string();
        let repo = git2::Repository::discover(path)
            .map_err(|_| Error::NotARepository(display))?;
        let root = repo
            .workdir()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| repo.path().to_path_buf());
        Ok(Self { repo, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_bare(&self) -> bool {
        self.repo.is_bare()
    }

    /// Name of the currently checked-out branch, if any.
    pub fn branch(&self) -> Option<String> {
        let head = self.repo.head().ok()?;
        if head.is_branch() {
            head.shorthand().map(str::to_string)
        } else {
            None
        }
    }

    /// True when HEAD has never been committed.
    pub fn is_unborn(&self) -> bool {
        self.repo.head().is_err()
    }

    pub fn head_revision(&self) -> Result<Revision> {
        if self.repo.head().is_err() {
            return Err(Error::UnknownRevision("HEAD (unborn)".into()));
        }
        let commit = self.repo.head()?.peel_to_commit()?;
        Ok(revision_from_commit(&commit))
    }

    /// Walk history from HEAD, newest first, up to `limit` revisions.
    pub fn revisions(&self, limit: usize) -> Result<Vec<Revision>> {
        if self.repo.head().is_err() {
            return Ok(Vec::new());
        }
        let mut walk = self.repo.revwalk()?;
        walk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)?;
        walk.push_head()?;

        let mut out = Vec::new();
        for oid in walk.take(limit) {
            let commit = self.repo.find_commit(oid?)?;
            out.push(revision_from_commit(&commit));
        }
        Ok(out)
    }

    /// List files in a revision's tree. `rev = None` means HEAD.
    pub fn files_at(&self, rev: Option<&str>) -> Result<Vec<FileEntry>> {
        let tree = self.tree_for(rev)?;
        let repo = &self.repo;
        let mut out: Vec<FileEntry> = Vec::new();

        tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                if let Some(name) = entry.name() {
                    let path = format!("{root}{name}");
                    if let Ok(blob) = repo.find_blob(entry.id()) {
                        out.push(FileEntry {
                            path,
                            oid: entry.id().to_string(),
                            size: blob.content().len() as u64,
                            is_binary: blob.is_binary(),
                        });
                    }
                }
            }
            git2::TreeWalkResult::Ok
        })?;

        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Read the raw bytes of a file at a revision. `rev = None` means HEAD.
    pub fn blob(&self, rev: Option<&str>, path: &str) -> Result<Vec<u8>> {
        let tree = self.tree_for(rev)?;
        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| Error::UnknownRevision(format!("{path} @ {}", rev.unwrap_or("HEAD"))))?;
        let blob = self.repo.find_blob(entry.id())?;
        Ok(blob.content().to_vec())
    }

    fn tree_for(&self, rev: Option<&str>) -> Result<git2::Tree<'_>> {
        match rev {
            Some(r) => Ok(self.repo.revparse_single(r)?.peel_to_tree()?),
            None => Ok(self.repo.head()?.peel_to_tree()?),
        }
    }
}

fn revision_from_commit(commit: &git2::Commit<'_>) -> Revision {
    let oid = commit.id().to_string();
    let author = commit.author();
    Revision {
        short: oid.chars().take(8).collect(),
        oid,
        summary: commit.summary().unwrap_or("(no message)").to_string(),
        author: author.name().unwrap_or("unknown").to_string(),
        email: author.email().unwrap_or("").to_string(),
        time: commit.time().seconds(),
        parents: commit.parent_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(dir: &Path) -> git2::Repository {
        let repo = git2::Repository::init(dir).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "Test").unwrap();
            cfg.set_str("user.email", "test@example.com").unwrap();
        }
        repo
    }

    fn commit_file(repo: &git2::Repository, name: &str, contents: &[u8], msg: &str) {
        std::fs::write(repo.workdir().unwrap().join(name), contents).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        let parents = match repo.head() {
            Ok(head) => vec![repo.find_commit(head.target().unwrap()).unwrap()],
            Err(_) => vec![],
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parent_refs)
            .unwrap();
    }

    #[test]
    fn lists_revisions_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        commit_file(&repo, "b.txt", b"two", "second");

        let project = Project::open(dir.path()).unwrap();
        let revs = project.revisions(10).unwrap();
        assert_eq!(revs.len(), 2);
        assert_eq!(revs[0].summary, "second");
        assert_eq!(revs[1].summary, "first");
        assert!(project.branch().is_some());
    }

    #[test]
    fn reads_files_and_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "part.stl", b"solid x", "add stl");

        let project = Project::open(dir.path()).unwrap();
        let files = project.files_at(None).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "part.stl");
        assert_eq!(project.blob(None, "part.stl").unwrap(), b"solid x");
    }
}
