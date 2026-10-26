use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Sentinel revision name selecting the *working tree* — the uncommitted
/// "Current" revision — instead of a commit. Passed through the same APIs that
/// otherwise take a commit oid/ref so previews resolve to on-disk contents.
pub const WORKTREE: &str = ":worktree:";

/// Project-local directory OpenPDM keeps machine-local state in: currently the
/// SQLite index. It is never project content, so it is hidden from every file
/// listing even when it is not gitignored.
///
/// Converters, by contrast, are app-level and live in the per-user
/// [`crate::app`] directory (`$HOME/.openpdm`), so they are shared across
/// projects. See [`crate::convert`].
pub const INTERNAL_DIR: &str = ".openpdm";

/// Patterns OpenPDM writes to a repository's *local* exclude file
/// (`.git/info/exclude`) so machine-local and editor-generated paths never show
/// up as changes. They are deliberately not written to a tracked `.gitignore`,
/// because they are OpenPDM/editor concerns rather than project content.
///
/// * `.openpdm/` — OpenPDM's own [`INTERNAL_DIR`] state (the SQLite index).
/// * `~$*` — CAD application lock files. SolidWorks and friends drop a
///   transitory `~$Block.SLDPRT` beside a document while it is open.
pub const LOCAL_EXCLUDES: &[&str] = &[".openpdm/", "~$*"];

/// Header written above [`LOCAL_EXCLUDES`] in `.git/info/exclude`.
const EXCLUDE_HEADER: &str = "# Added by OpenPDM: machine-local, never committed";

/// Whether `path` is OpenPDM's own [`INTERNAL_DIR`] or lives inside it.
///
/// Git paths are relative and slash-separated; a backslash form is normalised
/// too so a worktree path from a Windows platform check cannot slip through.
pub fn is_internal_path(path: &str) -> bool {
    path.replace('\\', "/")
        .split('/')
        .next()
        .is_some_and(|first| first == INTERNAL_DIR)
}

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
        let project = Self { repo, root };
        // Best-effort: refreshing the local excludes is a convenience, not a
        // precondition for reading a repository (which may be read-only).
        let _ = project.ensure_local_excludes();
        Ok(project)
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
        let project = Self { repo, root };
        project.ensure_local_excludes()?;
        Ok(project)
    }

    /// Make sure the repository's local exclude file lists [`LOCAL_EXCLUDES`].
    ///
    /// OpenPDM's machine-local state and CAD applications' transient lock files
    /// should never be committed. Writing the patterns to `.git/info/exclude`
    /// keeps `git status` clean in a terminal too, without adding a tracked
    /// `.gitignore` to the user's project. The file listings hide these paths on
    /// their own, so this is belt-and-braces for tooling that reads the
    /// repository directly.
    fn ensure_local_excludes(&self) -> Result<()> {
        if self.repo.is_bare() {
            return Ok(());
        }
        let exclude = self.repo.path().join("info").join("exclude");
        let existing = std::fs::read_to_string(&exclude).unwrap_or_default();
        let (has_header, missing): (bool, Vec<&str>) = {
            let present: BTreeSet<&str> = existing.lines().map(str::trim).collect();
            (
                present.contains(EXCLUDE_HEADER),
                LOCAL_EXCLUDES
                    .iter()
                    .copied()
                    .filter(|pattern| !present.contains(pattern))
                    .collect(),
            )
        };
        if missing.is_empty() {
            return Ok(());
        }
        if let Some(parent) = exclude.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut contents = existing;
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        if !has_header {
            contents.push_str(EXCLUDE_HEADER);
            contents.push('\n');
        }
        for pattern in missing {
            contents.push_str(pattern);
            contents.push('\n');
        }
        std::fs::write(&exclude, contents)?;
        Ok(())
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
                    if is_internal_path(&path) {
                        return git2::TreeWalkResult::Ok;
                    }
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
            if is_internal_path(path) {
                continue;
            }
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

    /// Every project file in the working tree (tracked and untracked), with the
    /// staging/`status` fields populated only for files that changed.
    ///
    /// Backs the Files panel's "show all files" view of the open "Current"
    /// revision, where unchanged files appear alongside the working changes.
    /// Untracked files are included; ignored files are not.
    pub fn all_worktree_files(&self) -> Result<Vec<FileEntry>> {
        if self.repo.is_bare() {
            return Ok(Vec::new());
        }
        // Changed files already carry the staging flag and change kind.
        let changed: HashMap<String, FileEntry> = self
            .working_files()?
            .into_iter()
            .map(|file| (file.path.clone(), file))
            .collect();

        // Tracked paths come from the index; untracked paths from status.
        let mut paths: BTreeSet<String> = BTreeSet::new();
        let index = self.repo.index()?;
        for entry in index.iter() {
            if let Ok(path) = std::str::from_utf8(&entry.path) {
                paths.insert(path.to_string());
            }
        }
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        for entry in self.repo.statuses(Some(&mut opts))?.iter() {
            if let Some(path) = entry.path() {
                paths.insert(path.to_string());
            }
        }

        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            if is_internal_path(&path) {
                continue;
            }
            if let Some(file) = changed.get(&path) {
                out.push(file.clone());
            } else {
                let (oid, size, is_binary) = self.worktree_blob_info(&path);
                out.push(FileEntry {
                    path,
                    oid,
                    size,
                    is_binary,
                    staged: None,
                    status: None,
                });
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// The files a commit changed relative to its first parent (or the empty
    /// tree for a root commit).
    ///
    /// Each entry carries the change `status` (`new`, `modified`, `deleted`,
    /// `renamed` or `typechange`); `staged` is left `None` because commits have
    /// no index/working state. Backs the Files panel's default view of a
    /// committed revision.
    pub fn changed_files_at(&self, rev: &str) -> Result<Vec<FileEntry>> {
        let commit = self.repo.revparse_single(rev)?.peel_to_commit()?;
        let tree = commit.tree()?;
        let parent_tree = match commit.parent(0) {
            Ok(parent) => Some(parent.tree()?),
            Err(_) => None,
        };
        let diff = self
            .repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None)?;

        let mut out: Vec<FileEntry> = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                let (path, kind) = match delta.status() {
                    git2::Delta::Added => (delta.new_file().path(), "new"),
                    git2::Delta::Deleted => (delta.old_file().path(), "deleted"),
                    git2::Delta::Renamed => (delta.new_file().path(), "renamed"),
                    git2::Delta::Typechange => (delta.new_file().path(), "typechange"),
                    _ => (delta.new_file().path(), "modified"),
                };
                if let Some(path) = path {
                    let path = path.to_string_lossy().replace('\\', "/");
                    if is_internal_path(&path) {
                        return true;
                    }
                    let (oid, size, is_binary) = self.tree_blob_info(&tree, &path);
                    out.push(FileEntry {
                        path,
                        oid,
                        size,
                        is_binary,
                        staged: None,
                        status: Some(kind.to_string()),
                    });
                }
                true
            },
            None,
            None,
            None,
        )?;

        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// The full tree at `rev`, annotated with `status` on the files the
    /// revision changed relative to its first parent.
    ///
    /// Backs the Files panel's "show all files" view of a committed revision:
    /// the committed changes are highlighted while the unchanged files that make
    /// up the rest of the tree are listed too.
    pub fn annotated_files_at(&self, rev: &str) -> Result<Vec<FileEntry>> {
        let mut files = self.files_at(Some(rev))?;
        let mut statuses: HashMap<String, String> = self
            .changed_files_at(rev)?
            .into_iter()
            .filter_map(|file| file.status.map(|status| (file.path, status)))
            .collect();
        for file in &mut files {
            if let Some(status) = statuses.remove(&file.path) {
                file.status = Some(status);
            }
        }
        Ok(files)
    }

    /// Commits that changed `path`, newest first, up to `limit`.
    ///
    /// Walks history from HEAD and keeps the commits whose diff against their
    /// first parent touches `path`. Backs the per-file Revisions list in the
    /// detail panel, where each entry is a committed version of the part. A
    /// file that no commit has touched yet (e.g. a new, untracked file) has an
    /// empty history rather than an error.
    pub fn file_revisions(&self, path: &str, limit: usize) -> Result<Vec<Revision>> {
        if self.repo.head().is_err() {
            return Ok(Vec::new());
        }
        let mut walk = self.repo.revwalk()?;
        walk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)?;
        walk.push_head()?;

        // Restrict every commit's diff to the path we care about.
        let mut opts = git2::DiffOptions::new();
        opts.pathspec(path);

        let mut out = Vec::new();
        for oid in walk {
            if out.len() >= limit {
                break;
            }
            let commit = self.repo.find_commit(oid?)?;
            let tree = commit.tree()?;
            let parent_tree = match commit.parent(0) {
                Ok(parent) => Some(parent.tree()?),
                Err(_) => None,
            };
            let diff = self
                .repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;
            if diff.deltas().len() > 0 {
                out.push(revision_from_commit(&commit));
            }
        }
        Ok(out)
    }

    /// Size, binary flag and content id for `path` in a committed tree. Empty
    /// when the path is absent (e.g. a file the commit deleted).
    fn tree_blob_info(&self, tree: &git2::Tree<'_>, path: &str) -> (String, u64, bool) {
        if let Ok(entry) = tree.get_path(Path::new(path)) {
            if let Ok(blob) = self.repo.find_blob(entry.id()) {
                return (
                    entry.id().to_string(),
                    blob.content().len() as u64,
                    blob.is_binary(),
                );
            }
        }
        (String::new(), 0, false)
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

    /// Restore `path` from `rev` into the working tree and index.
    ///
    /// The equivalent of `git checkout <rev> -- <path>`: only `path` is
    /// touched, so a previous version of a single part can be brought back
    /// without moving HEAD. The restored content is staged, so it shows up as a
    /// change in the open "Current" revision.
    pub fn checkout_file(&self, rev: &str, path: &str) -> Result<()> {
        if self.repo.is_bare() {
            return Err(Error::NoWorktree(
                "cannot check out files into a bare repository".into(),
            ));
        }
        let rel = Path::new(path);
        // Keep the write inside the repository root.
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::UnknownRevision(path.to_string()));
        }

        let commit = self.repo.revparse_single(rev)?.peel_to_commit()?;
        let tree = commit.tree()?;
        let entry = tree
            .get_path(rel)
            .map_err(|_| Error::UnknownRevision(format!("{path} @ {rev}")))?;
        let blob = self.repo.find_blob(entry.id())?;

        let dest = self.root.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, blob.content())?;

        // Stage the restored content so the index matches the working tree.
        let mut index = self.repo.index()?;
        index.add_path(rel)?;
        index.write()?;
        Ok(())
    }

    /// Discard the working-tree change to `path`, restoring it to HEAD.
    ///
    /// The equivalent of `git checkout HEAD -- <path>`: a modified or deleted
    /// tracked file is restored to its committed content, while a new file is
    /// removed from disk. The index entry is synced to HEAD too, so the path
    /// stops showing up as a change in the open "Current" revision.
    pub fn revert(&self, path: &str) -> Result<()> {
        if self.repo.is_bare() {
            return Err(Error::NoWorktree(
                "cannot revert files in a bare repository".into(),
            ));
        }
        let rel = Path::new(path);
        // Keep the write inside the repository root.
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::UnknownRevision(path.to_string()));
        }

        // The committed blob for `path`, if HEAD tracks it at all.
        let head_oid = match self.repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit()?;
                let tree = commit.tree()?;
                tree.get_path(rel).ok().map(|entry| entry.id())
            }
            Err(_) => None,
        };

        let dest = self.root.join(rel);
        let mut index = self.repo.index()?;
        match head_oid {
            // Tracked in HEAD: put the committed content back and sync the index.
            Some(oid) => {
                let blob = self.repo.find_blob(oid)?;
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&dest, blob.content())?;
                index.add_path(rel)?;
            }
            // New file: drop it from disk and from the index.
            None => {
                if dest.exists() {
                    std::fs::remove_file(&dest)?;
                }
                index.remove_path(rel)?;
            }
        }
        index.write()?;
        Ok(())
    }

    /// Remove a single file's version from history.
    ///
    /// Rewrites the target commit so it no longer changes `path` (reverting the
    /// file to its state in the commit's first parent), then rewrites the
    /// commits that follow on the current branch so the file's later versions
    /// are preserved. Only `path` is touched: every other file in the rewritten
    /// commits keeps its original contents. The working tree and index are then
    /// synced to the rewritten tip for `path`, so the repository is left clean.
    ///
    /// Because rewriting changes the oids of the target commit and everything
    /// after it, callers must refresh their revision and file lists afterwards.
    pub fn delete_file_revision(&self, rev: &str, path: &str) -> Result<()> {
        let rel = Path::new(path);
        // Keep every read/write inside the repository root.
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(Error::UnknownRevision(path.to_string()));
        }
        if self.repo.head().is_err() {
            return Err(Error::UnknownRevision("HEAD (unborn)".into()));
        }

        let target = self.repo.revparse_single(rev)?.peel_to_commit()?;
        let head = self.repo.head()?.peel_to_commit()?;

        // Walk every commit reachable from HEAD, oldest first, so a parent's
        // rewritten oid is known before its children are processed.
        let mut walk = self.repo.revwalk()?;
        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)?;
        walk.push_head()?;

        // old oid -> rewritten oid (identity for commits before the target).
        let mut map: HashMap<git2::Oid, git2::Oid> = HashMap::new();
        // old oid -> whether the commit is the target or descends from it.
        let mut affected: HashMap<git2::Oid, bool> = HashMap::new();

        for oid in walk {
            let oid = oid?;
            let commit = self.repo.find_commit(oid)?;
            let is_affected = oid == target.id()
                || commit
                    .parent_ids()
                    .any(|pid| affected.get(&pid).copied().unwrap_or(false));

            if !is_affected {
                map.insert(oid, oid);
                affected.insert(oid, false);
                continue;
            }

            let tree = commit.tree()?;
            let original_value = self.tree_entry_value(&tree, rel)?;
            // The value the first parent will have after rewriting (if any).
            let parent_value = match commit.parent_ids().next() {
                Some(pid) => {
                    let new_pid = map.get(&pid).copied().unwrap_or(pid);
                    self.path_entry_in_commit(new_pid, rel)?
                }
                None => None,
            };

            // The target reverts the file; a descendant that changed the path
            // keeps its own version; anything else carries the parent forward.
            let new_value = if oid == target.id() || !self.commit_changes_path(&commit, rel)? {
                parent_value
            } else {
                original_value
            };

            // Reuse the original tree when the path is unchanged so only the
            // parent link (and therefore the commit oid) moves.
            let new_tree_id = if new_value == original_value {
                commit.tree_id()
            } else {
                self.tree_with_path(&tree, rel, new_value)?
            };
            let new_tree = self.repo.find_tree(new_tree_id)?;

            let new_parents: Vec<git2::Commit> = commit
                .parent_ids()
                .map(|pid| {
                    let new_pid = map.get(&pid).copied().unwrap_or(pid);
                    self.repo.find_commit(new_pid)
                })
                .collect::<std::result::Result<_, _>>()?;
            let parent_refs: Vec<&git2::Commit> = new_parents.iter().collect();

            let new_oid = self.repo.commit(
                None,
                &commit.author(),
                &commit.committer(),
                commit.message().unwrap_or(""),
                &new_tree,
                &parent_refs,
            )?;
            map.insert(oid, new_oid);
            affected.insert(oid, true);
        }

        let new_head = map.get(&head.id()).copied().unwrap_or(head.id());
        if new_head == head.id() {
            // The target is not on the current branch, so nothing was rewritten.
            return Err(Error::UnknownRevision(format!("{path} @ {rev}")));
        }

        // Move the checked-out branch (or detached HEAD) to the rewritten tip.
        let branch = self.repo.head().ok().and_then(|h| {
            if h.is_branch() {
                h.name().map(str::to_string)
            } else {
                None
            }
        });
        match branch {
            Some(name) => {
                self.repo
                    .reference(&name, new_head, true, "openpdm: delete file revision")?;
            }
            None => self.repo.set_head_detached(new_head)?,
        }

        // Sync `path` in the working tree and index to the rewritten tip so the
        // deletion does not leave a phantom change behind.
        self.sync_worktree_path(path)?;
        Ok(())
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
            None => {
                // A freshly-initialised repository has an unborn HEAD. `files_at`
                // degrades to "no files" in that case, but reading a blob at HEAD
                // cannot: explain it instead of leaking git2's UnbornBranch.
                if self.is_unborn() {
                    return Err(Error::UnknownRevision(
                        "HEAD is unborn (no commits yet) — use --rev :worktree: for the working tree"
                            .into(),
                    ));
                }
                Ok(self.repo.head()?.peel_to_tree()?)
            }
        }
    }

    /// The blob oid and file mode of `path` in `tree`, if present.
    fn tree_entry_value(
        &self,
        tree: &git2::Tree<'_>,
        path: &Path,
    ) -> Result<Option<(git2::Oid, i32)>> {
        match tree.get_path(path) {
            Ok(entry) => Ok(Some((entry.id(), entry.filemode()))),
            Err(_) => Ok(None),
        }
    }

    /// The blob oid and file mode of `path` in a commit's tree, if present.
    fn path_entry_in_commit(
        &self,
        commit_oid: git2::Oid,
        path: &Path,
    ) -> Result<Option<(git2::Oid, i32)>> {
        let commit = self.repo.find_commit(commit_oid)?;
        self.tree_entry_value(&commit.tree()?, path)
    }

    /// Whether `commit` changed `path` relative to its first parent.
    fn commit_changes_path(&self, commit: &git2::Commit<'_>, path: &Path) -> Result<bool> {
        let tree = commit.tree()?;
        let parent_tree = match commit.parent(0) {
            Ok(parent) => Some(parent.tree()?),
            Err(_) => None,
        };
        let mut opts = git2::DiffOptions::new();
        opts.pathspec(path);
        let diff = self
            .repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;
        Ok(diff.deltas().len() > 0)
    }

    /// A copy of `tree` with `path` set to `value` (or removed when `None`).
    ///
    /// Builds the new tree through an in-memory index so nested paths are
    /// handled without walking the tree by hand.
    fn tree_with_path(
        &self,
        tree: &git2::Tree<'_>,
        path: &Path,
        value: Option<(git2::Oid, i32)>,
    ) -> Result<git2::Oid> {
        let mut index = git2::Index::new()?;
        index.read_tree(tree)?;
        match value {
            Some((oid, mode)) => {
                let entry = git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: mode as u32,
                    uid: 0,
                    gid: 0,
                    file_size: 0,
                    id: oid,
                    flags: 0,
                    flags_extended: 0,
                    path: path.to_string_lossy().replace('\\', "/").into_bytes(),
                };
                index.add(&entry)?;
            }
            None => {
                if index.get_path(path, 0).is_some() {
                    index.remove_path(path)?;
                }
            }
        }
        Ok(index.write_tree_to(&self.repo)?)
    }

    /// Point the working tree and index at `path`'s content in HEAD.
    fn sync_worktree_path(&self, path: &str) -> Result<()> {
        if self.repo.is_bare() {
            return Ok(());
        }
        let rel = Path::new(path);
        let tree = self.repo.head()?.peel_to_tree()?;
        match tree.get_path(rel) {
            Ok(entry) => {
                let blob = self.repo.find_blob(entry.id())?;
                let dest = self.root.join(rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&dest, blob.content())?;
                let mut index = self.repo.index()?;
                index.add_path(rel)?;
                index.write()?;
            }
            Err(_) => {
                let dest = self.root.join(rel);
                if dest.exists() {
                    std::fs::remove_file(&dest)?;
                }
                let mut index = self.repo.index()?;
                if index.get_path(rel, 0).is_some() {
                    index.remove_path(rel)?;
                    index.write()?;
                }
            }
        }
        Ok(())
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
        let dest = repo.workdir().unwrap().join(name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(dest, contents).unwrap();
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
    fn reading_head_on_an_unborn_repo_is_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        let project = Project::init(dir.path()).unwrap();

        // `files_at` degrades to "no files" for an unborn HEAD, but reading a
        // blob at HEAD must explain the situation rather than leaking git2's
        // UnbornBranch.
        let err = project.blob(None, "part.sldprt").unwrap_err().to_string();
        assert!(err.contains("unborn"), "unexpected error: {err}");
        assert!(err.contains(":worktree:"), "unexpected error: {err}");
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
    fn reverts_modified_and_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        let old = repo.find_commit(repo.refname_to_id("HEAD").unwrap()).unwrap();
        let old_tree = old.tree().unwrap();

        // Modify the tracked file and add an untracked one, staging both.
        std::fs::write(dir.path().join("a.txt"), b"one changed").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"two").unwrap();

        let project = Project::open(dir.path()).unwrap();
        project.stage("a.txt").unwrap();
        project.stage("b.txt").unwrap();

        // Reverting the modified file restores its committed content and drops
        // it from the working changes.
        project.revert("a.txt").unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("a.txt")).unwrap(),
            b"one".to_vec()
        );
        assert!(!project
            .working_files()
            .unwrap()
            .iter()
            .any(|f| f.path == "a.txt"));

        // Reverting the new file removes it from disk.
        project.revert("b.txt").unwrap();
        assert!(!dir.path().join("b.txt").exists());
        assert!(project.working_files().unwrap().is_empty());

        // The index is back in sync with HEAD.
        let index_tree = project.repo.index().unwrap().write_tree().unwrap();
        assert_eq!(index_tree, old_tree.id());
    }

    #[test]
    fn lists_all_worktree_files() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        commit_file(&repo, "b.txt", b"two", "second");

        // Modify one tracked file and add one untracked file.
        std::fs::write(dir.path().join("a.txt"), b"one changed").unwrap();
        std::fs::write(dir.path().join("c.txt"), b"three").unwrap();

        let project = Project::open(dir.path()).unwrap();
        let files = project.all_worktree_files().unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.txt", "b.txt", "c.txt"]);

        // A working change keeps its staging/status fields.
        let a = files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(a.staged, Some(false));
        assert_eq!(a.status.as_deref(), Some("modified"));
        // An unchanged committed file is listed without a change.
        let b = files.iter().find(|f| f.path == "b.txt").unwrap();
        assert_eq!(b.staged, None);
        assert_eq!(b.status, None);
        // An untracked file is listed as a new change.
        let c = files.iter().find(|f| f.path == "c.txt").unwrap();
        assert_eq!(c.staged, Some(false));
        assert_eq!(c.status.as_deref(), Some("new"));
    }

    #[test]
    fn lists_a_commits_changes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        commit_file(&repo, "b.txt", b"two", "second");

        std::fs::write(dir.path().join("a.txt"), b"one changed").unwrap();
        let project = Project::open(dir.path()).unwrap();
        project.stage("a.txt").unwrap();
        let revision = project.commit("update a").unwrap();

        let files = project.changed_files_at(&revision.oid).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].status.as_deref(), Some("modified"));
        assert_eq!(files[0].size, b"one changed".len() as u64);
        assert_eq!(files[0].staged, None);
    }

    #[test]
    fn annotates_a_commits_tree_with_its_changes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "first");
        commit_file(&repo, "b.txt", b"two", "second");

        std::fs::write(dir.path().join("a.txt"), b"one changed").unwrap();
        let project = Project::open(dir.path()).unwrap();
        project.stage("a.txt").unwrap();
        let revision = project.commit("update a").unwrap();

        let files = project.annotated_files_at(&revision.oid).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.txt", "b.txt"]);
        // The changed file is annotated; the untouched one is not.
        assert_eq!(
            files
                .iter()
                .find(|f| f.path == "a.txt")
                .unwrap()
                .status
                .as_deref(),
            Some("modified")
        );
        assert_eq!(files.iter().find(|f| f.path == "b.txt").unwrap().status, None);
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

    #[test]
    fn lists_a_files_revisions_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "add a");
        commit_file(&repo, "b.txt", b"two", "add b");
        commit_file(&repo, "a.txt", b"one changed", "update a");

        let project = Project::open(dir.path()).unwrap();
        let revs = project.file_revisions("a.txt", 10).unwrap();
        assert_eq!(revs.len(), 2);
        assert_eq!(revs[0].summary, "update a");
        assert_eq!(revs[1].summary, "add a");

        // The limit is respected.
        assert_eq!(project.file_revisions("a.txt", 1).unwrap().len(), 1);
        // A file no commit touched has no history rather than an error.
        assert!(project.file_revisions("missing.txt", 10).unwrap().is_empty());
    }

    #[test]
    fn checks_out_a_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"one", "add a");
        let first = Project::open(dir.path()).unwrap().head_revision().unwrap();
        commit_file(&repo, "b.txt", b"two", "add b");
        commit_file(&repo, "a.txt", b"one changed", "update a");

        let project = Project::open(dir.path()).unwrap();
        project.checkout_file(&first.oid, "a.txt").unwrap();

        // The working file is restored to the first revision's content ...
        assert_eq!(project.blob(Some(WORKTREE), "a.txt").unwrap(), b"one");
        // ... and only that file: b.txt is untouched.
        assert_eq!(project.blob(Some(WORKTREE), "b.txt").unwrap(), b"two");
        // The restored content is staged as a change in the Current revision.
        let changes = project.working_files().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "a.txt");
        assert_eq!(changes[0].staged, Some(true));

        // Checking out a path absent from the revision is an error.
        assert!(project.checkout_file(&first.oid, "missing.txt").is_err());
    }

    #[test]
    fn deletes_a_files_revision_and_rewrites_descendants() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"v1", "add a");
        commit_file(&repo, "b.txt", b"other", "add b");
        commit_file(&repo, "a.txt", b"v2", "change a to v2");
        commit_file(&repo, "a.txt", b"v3", "change a to v3");

        let project = Project::open(dir.path()).unwrap();
        let revs = project.file_revisions("a.txt", 10).unwrap();
        assert_eq!(revs.len(), 3);
        let v2 = revs[1].oid.clone();

        project.delete_file_revision(&v2, "a.txt").unwrap();

        // The v2 revision is gone; the add and the newest version survive.
        let summaries: Vec<String> = project
            .file_revisions("a.txt", 10)
            .unwrap()
            .into_iter()
            .map(|r| r.summary)
            .collect();
        assert_eq!(summaries, vec!["change a to v3", "add a"]);

        // The tip still holds v3, the unrelated file is untouched, and the
        // working tree/index were synced so nothing looks changed.
        assert_eq!(project.blob(None, "a.txt").unwrap(), b"v3");
        assert_eq!(project.blob(None, "b.txt").unwrap(), b"other");
        assert!(project.working_files().unwrap().is_empty());
    }

    #[test]
    fn deleting_the_newest_revision_reverts_the_tip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"v1", "add a");
        commit_file(&repo, "a.txt", b"v2", "change a to v2");

        let project = Project::open(dir.path()).unwrap();
        let newest = project.file_revisions("a.txt", 10).unwrap()[0].oid.clone();
        project.delete_file_revision(&newest, "a.txt").unwrap();

        let revs = project.file_revisions("a.txt", 10).unwrap();
        assert_eq!(revs.len(), 1);
        assert_eq!(revs[0].summary, "add a");
        // HEAD and the working tree now hold the older content.
        assert_eq!(project.blob(None, "a.txt").unwrap(), b"v1");
        assert_eq!(project.blob(Some(WORKTREE), "a.txt").unwrap(), b"v1");
        assert!(project.working_files().unwrap().is_empty());
    }

    #[test]
    fn deleting_the_add_revision_preserves_later_versions() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", b"v1", "add a");
        commit_file(&repo, "a.txt", b"v2", "change a to v2");

        let project = Project::open(dir.path()).unwrap();
        let add = project.file_revisions("a.txt", 10).unwrap()[1].oid.clone();
        project.delete_file_revision(&add, "a.txt").unwrap();

        // The add is gone; the later version is now the first appearance.
        let revs = project.file_revisions("a.txt", 10).unwrap();
        assert_eq!(revs.len(), 1);
        assert_eq!(revs[0].summary, "change a to v2");
        assert_eq!(project.blob(None, "a.txt").unwrap(), b"v2");
        assert!(project.working_files().unwrap().is_empty());
    }

    #[test]
    fn classifies_internal_paths() {
        assert!(is_internal_path(".openpdm"));
        assert!(is_internal_path(".openpdm/"));
        assert!(is_internal_path(".openpdm/cache/stl/conv/abc.stl"));
        assert!(is_internal_path(".openpdm\\cache\\stl\\abc.stl"));
        // Only the root `.openpdm` is internal, and only as a whole component.
        assert!(!is_internal_path("parts/.openpdm/abc.stl"));
        assert!(!is_internal_path(".openpdm-notes/abc.stl"));
        assert!(!is_internal_path("part.stl"));
    }

    #[test]
    fn hides_the_internal_dir_from_worktree_listings() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "part.stl", b"solid x", "add part");

        // A cached conversion shares the source's project extension, so it must
        // not surface as a commit-worthy file.
        let cache = dir
            .path()
            .join(".openpdm")
            .join("cache")
            .join("stl")
            .join("conv");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("abc.stl"), b"solid cached").unwrap();
        std::fs::write(dir.path().join("bracket.stl"), b"solid y").unwrap();

        let project = Project::open(dir.path()).unwrap();
        let changes: Vec<String> = project
            .working_files()
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(changes, vec!["bracket.stl"]);

        let all: Vec<String> = project
            .all_worktree_files()
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(all, vec!["bracket.stl", "part.stl"]);
    }

    #[test]
    fn hides_the_internal_dir_from_committed_trees() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "part.stl", b"solid x", "add part");
        // Force an internal file into history to prove the listings hide it.
        commit_file(
            &repo,
            ".openpdm/cache/stl/conv/abc.stl",
            b"solid cached",
            "cache",
        );

        let project = Project::open(dir.path()).unwrap();
        let paths: Vec<String> = project
            .files_at(None)
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert_eq!(paths, vec!["part.stl"]);

        let changed: Vec<String> = project
            .changed_files_at("HEAD")
            .unwrap()
            .into_iter()
            .map(|f| f.path)
            .collect();
        assert!(changed.is_empty(), "unexpected changes: {changed:?}");
    }

    #[test]
    fn init_excludes_the_internal_dir_locally() {
        let dir = tempfile::tempdir().unwrap();
        let project = Project::init(dir.path()).unwrap();

        // The local exclude patterns are written to the repo-local exclude file
        // (not committed): OpenPDM state and CAD application lock files.
        let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
        assert!(exclude.lines().any(|line| line.trim() == ".openpdm/"));
        assert!(exclude.lines().any(|line| line.trim() == "~$*"));

        // A cache file is therefore ignored by git status and never a change.
        let cache = dir
            .path()
            .join(".openpdm")
            .join("cache")
            .join("stl")
            .join("conv");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("abc.stl"), b"solid cached").unwrap();
        assert!(project.working_files().unwrap().is_empty());

        // Re-initialising is idempotent: the patterns are not duplicated.
        Project::init(dir.path()).unwrap();
        let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
        for pattern in [".openpdm/", "~$*"] {
            assert_eq!(
                exclude.lines().filter(|line| line.trim() == pattern).count(),
                1,
                "pattern {pattern:?} duplicated"
            );
        }
    }

    #[test]
    fn opening_excludes_cad_lock_files_locally() {
        let dir = tempfile::tempdir().unwrap();
        // A repository created without OpenPDM, so only `open` can fix it up.
        let repo = init_repo(dir.path());
        commit_file(&repo, "part.stl", b"solid", "first");

        // SolidWorks' lock file for the open document.
        std::fs::write(dir.path().join("~$part.SLDPRT"), b"lock").unwrap();

        let project = Project::open(dir.path()).unwrap();

        // The pattern is added to the repo-local exclude on open, so the lock
        // file is not a working change and never listed.
        let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
        assert!(exclude.lines().any(|line| line.trim() == "~$*"));
        assert!(project.working_files().unwrap().is_empty());

        // Opening again is idempotent.
        Project::open(dir.path()).unwrap();
        let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
        assert_eq!(
            exclude.lines().filter(|line| line.trim() == "~$*").count(),
            1
        );
    }
}
