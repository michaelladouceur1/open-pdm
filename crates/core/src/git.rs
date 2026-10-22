use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Sentinel revision name selecting the *working tree* — the uncommitted
/// "Current" revision — instead of a commit. Passed through the same APIs that
/// otherwise take a commit oid/ref so previews resolve to on-disk contents.
pub const WORKTREE: &str = ":worktree:";

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
///
/// For the committed history the staging fields are `None`. When listing the
/// working tree (the open ["Current"](WORKTREE) revision) they describe how the
/// change would be or is already part of the revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    /// Git blob object id (content address). Empty for a working change whose
    /// content is neither in the index nor in HEAD (e.g. an unstaged deletion).
    pub oid: String,
    pub size: u64,
    pub is_binary: bool,
    /// Whether the working change is staged into the open revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged: Option<bool>,
    /// Working change kind: `new`, `modified`, `deleted`, `renamed` or
    /// `typechange`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// A thin, git-backed view of a project.
///
/// Git is the source of truth. This type reads history and can initialise an
/// empty repository ([`Project::init`]); content writes, network and LFS/lock
/// operations are intentionally left to the `git`/`git-lfs` subprocess layer
/// (not part of this first pass).
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

    /// Initialise a new git repository rooted at `path` and return a view of it.
    ///
    /// Used by the desktop app's "New" flow. If a repository already exists at
    /// (or above) `path`, the existing repository is (re)initialised in place
    /// rather than failing, so creating a project is idempotent.
    pub fn init(path: impl AsRef<Path>) -> Result<Self> {
        let repo = git2::Repository::init(path)?;
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
    ///
    /// A freshly-initialised repository has an unborn HEAD; it simply has no
    /// files yet rather than being an error.
    pub fn files_at(&self, rev: Option<&str>) -> Result<Vec<FileEntry>> {
        if rev.is_none() && self.is_unborn() {
            return Ok(Vec::new());
        }
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
                            staged: None,
                            status: None,
                        });
                    }
                }
            }
            git2::TreeWalkResult::Ok
        })?;

        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Read the raw bytes of a file at a revision. `rev = None` means HEAD;
    /// [`WORKTREE`] reads the on-disk file from the open "Current" revision.
    pub fn blob(&self, rev: Option<&str>, path: &str) -> Result<Vec<u8>> {
        if rev == Some(WORKTREE) {
            let rel = Path::new(path);
            // Keep worktree reads inside the repository root.
            if rel.is_absolute()
                || rel
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(Error::UnknownRevision(path.to_string()));
            }
            return Ok(std::fs::read(self.root.join(rel))?);
        }
        let tree = self.tree_for(rev)?;
        let entry = tree
            .get_path(Path::new(path))
            .map_err(|_| Error::UnknownRevision(format!("{path} @ {}", rev.unwrap_or("HEAD"))))?;
        let blob = self.repo.find_blob(entry.id())?;
        Ok(blob.content().to_vec())
    }

    /// The changed files that make up the open "Current" revision: everything
    /// that differs between the worktree/index and HEAD.
    ///
    /// Untracked files are included (a new part is a change); ignored files are
    /// not. Each entry reports whether it is already staged into the revision.
    pub fn working_files(&self) -> Result<Vec<FileEntry>> {
        if self.repo.is_bare() {
            return Ok(Vec::new());
        }
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false)
            .renames_head_to_index(true)
            .renames_index_to_workdir(true);

        let statuses = self.repo.statuses(Some(&mut opts))?;
        let mut out = Vec::with_capacity(statuses.len());
        for entry in statuses.iter() {
            let Some(path) = entry.path() else { continue };
            let status = entry.status();
            let (oid, size, is_binary) = self.worktree_blob_info(path);
            out.push(FileEntry {
                path: path.to_string(),
                oid,
                size,
                is_binary,
                staged: Some(is_staged(status)),
                status: Some(change_kind(status).to_string()),
            });
        }

        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Stage a working change into the open "Current" revision (git's index).
    ///
    /// A file present on disk is added from the worktree; a missing file is
    /// staged as a deletion.
    pub fn stage(&self, path: &str) -> Result<()> {
        let mut index = self.repo.index()?;
        let path = Path::new(path);
        if self.root.join(path).exists() {
            index.add_path(path)?;
        } else {
            index.remove_path(path)?;
        }
        index.write()?;
        Ok(())
    }

    /// Remove a staged change from the open revision, restoring the index entry
    /// from HEAD. A file that is new (and therefore absent from HEAD) is
    /// unstaged entirely.
    pub fn unstage(&self, path: &str) -> Result<()> {
        let target = match self.repo.head() {
            Ok(head) => head.peel(git2::ObjectType::Commit).ok(),
            Err(_) => None,
        };
        self.repo.reset_default(target.as_ref(), [path])?;
        Ok(())
    }

    /// Commit the staged changes of the open "Current" revision.
    ///
    /// The commit is built from the current index tree and made the new tip of
    /// the checked-out branch (or HEAD when detached). An empty message and an
    /// index that matches HEAD are both rejected, the latter as
    /// [`Error::NothingStaged`], so the UI never produces a no-op commit.
    pub fn commit(&self, message: &str) -> Result<Revision> {
        let message = message.trim();
        if message.is_empty() {
            return Err(Error::EmptyMessage);
        }

        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let parent = match self.repo.head() {
            Ok(head) => Some(head.peel_to_commit()?),
            Err(_) => None,
        };

        // Nothing staged: the index tree either matches HEAD or, on an unborn
        // branch, is empty. Creating a commit would be a no-op.
        let has_changes = match &parent {
            Some(parent) => parent.tree_id() != tree_id,
            None => !tree.is_empty(),
        };
        if !has_changes {
            return Err(Error::NothingStaged);
        }

        let signature = self.repo.signature()?;
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let oid = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )?;
        let commit = self.repo.find_commit(oid)?;
        Ok(revision_from_commit(&commit))
    }

    /// Size, binary flag and content id for a working file, preferring the
    /// on-disk contents and falling back to the staged index entry (for, say, a
    /// staged then deleted file).
    fn worktree_blob_info(&self, path: &str) -> (String, u64, bool) {
        if let Ok(bytes) = std::fs::read(self.root.join(path)) {
            // Mirror git's own heuristic: binary when a NUL appears in the
            // first 8000 bytes.
            let is_binary = bytes.iter().take(8000).any(|&b| b == 0);
            let oid = git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)
                .map(|oid| oid.to_string())
                .unwrap_or_default();
            return (oid, bytes.len() as u64, is_binary);
        }
        if let Ok(index) = self.repo.index() {
            if let Some(entry) = index.get_path(Path::new(path), 0) {
                if let Ok(blob) = self.repo.find_blob(entry.id) {
                    return (entry.id.to_string(), blob.content().len() as u64, blob.is_binary());
                }
            }
        }
        (String::new(), 0, false)
    }

    fn tree_for(&self, rev: Option<&str>) -> Result<git2::Tree<'_>> {
        match rev {
            Some(r) => Ok(self.repo.revparse_single(r)?.peel_to_tree()?),
            None => Ok(self.repo.head()?.peel_to_tree()?),
        }
    }
}

/// True when the status carries a change that has been staged into the index.
fn is_staged(status: git2::Status) -> bool {
    status.intersects(
        git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE,
    )
}

/// Classify a status as one of the working-change kinds the UI understands.
fn change_kind(status: git2::Status) -> &'static str {
    use git2::Status;
    if status.intersects(Status::INDEX_NEW | Status::WT_NEW) {
        "new"
    } else if status.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
        "deleted"
    } else if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
        "renamed"
    } else if status.intersects(Status::INDEX_TYPECHANGE | Status::WT_TYPECHANGE) {
        "typechange"
    } else {
        "modified"
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
    fn init_creates_repository() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("new-project");

        let project = Project::init(&nested).unwrap();

        assert!(nested.join(".git").is_dir());
        assert_eq!(project.root(), nested.as_path());
        assert!(project.is_unborn());
        assert_eq!(project.revisions(10).unwrap().len(), 0);
        // An unborn repo has no files; this must not error.
        assert!(project.files_at(None).unwrap().is_empty());
        assert!(project.head_revision().is_err());
    }

    #[test]
    fn init_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        Project::init(dir.path()).unwrap();
        let project = Project::init(dir.path()).unwrap();
        assert!(project.root().join(".git").is_dir());
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
    fn lists_working_changes_then_stages_and_unstages() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");

        // Modify a tracked file and add an untracked one.
        std::fs::write(dir.path().join("a.txt"), b"one changed").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"two").unwrap();

        let project = Project::open(dir.path()).unwrap();
        let files = project.working_files().unwrap();
        assert_eq!(files.len(), 2);
        let a = files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(a.staged, Some(false));
        assert_eq!(a.status.as_deref(), Some("modified"));
        assert_eq!(a.size, b"one changed".len() as u64);
        let b = files.iter().find(|f| f.path == "b.txt").unwrap();
        assert_eq!(b.staged, Some(false));
        assert_eq!(b.status.as_deref(), Some("new"));

        project.stage("a.txt").unwrap();
        project.stage("b.txt").unwrap();
        assert!(project
            .working_files()
            .unwrap()
            .iter()
            .all(|f| f.staged == Some(true)));

        project.unstage("a.txt").unwrap();
        let files = project.working_files().unwrap();
        assert_eq!(
            files.iter().find(|f| f.path == "a.txt").unwrap().staged,
            Some(false)
        );
        assert_eq!(
            files.iter().find(|f| f.path == "b.txt").unwrap().staged,
            Some(true)
        );

        // Unstaging a new file drops it from the index entirely.
        project.unstage("b.txt").unwrap();
        let b = project
            .working_files()
            .unwrap()
            .into_iter()
            .find(|f| f.path == "b.txt")
            .unwrap();
        assert_eq!(b.staged, Some(false));
        assert_eq!(b.status.as_deref(), Some("new"));
    }

    #[test]
    fn commits_staged_changes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        std::fs::write(dir.path().join("a.txt"), b"one changed").unwrap();

        let project = Project::open(dir.path()).unwrap();
        // Nothing staged yet, so a commit is a no-op and must be refused.
        assert!(matches!(project.commit("nope"), Err(Error::NothingStaged)));

        project.stage("a.txt").unwrap();
        let revision = project.commit("update a").unwrap();
        assert_eq!(revision.summary, "update a");
        assert_eq!(project.revisions(10).unwrap()[0].oid, revision.oid);
        // The index now matches HEAD, so the working tree is clean.
        assert!(project.working_files().unwrap().is_empty());
    }

    #[test]
    fn commits_first_revision_on_unborn_branch() {
        let dir = tempfile::tempdir().unwrap();
        let _repo = init_repo(dir.path());

        let project = Project::open(dir.path()).unwrap();
        assert!(project.is_unborn());
        // An empty index has nothing to commit.
        assert!(matches!(project.commit("first"), Err(Error::NothingStaged)));

        std::fs::write(dir.path().join("part.stl"), b"solid x").unwrap();
        project.stage("part.stl").unwrap();
        let revision = project.commit("first").unwrap();
        assert_eq!(revision.summary, "first");
        assert_eq!(revision.parents, 0);
        assert!(!project.is_unborn());
        assert_eq!(project.revisions(10).unwrap().len(), 1);
    }

    #[test]
    fn rejects_empty_commit_message() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        std::fs::write(dir.path().join("a.txt"), b"one changed").unwrap();

        let project = Project::open(dir.path()).unwrap();
        project.stage("a.txt").unwrap();
        assert!(matches!(project.commit("   "), Err(Error::EmptyMessage)));
        // The staged change is left intact for a corrected message.
        assert_eq!(project.working_files().unwrap().len(), 1);
    }

    #[test]
    fn reads_worktree_blob() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        std::fs::write(dir.path().join("a.txt"), b"changed").unwrap();

        let project = Project::open(dir.path()).unwrap();
        assert_eq!(project.blob(Some(WORKTREE), "a.txt").unwrap(), b"changed");
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
