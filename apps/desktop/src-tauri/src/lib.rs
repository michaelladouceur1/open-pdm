//! Tauri host for the OpenPDM desktop app.
//!
//! The host is deliberately thin: it exposes the core (git history, file
//! metadata, tessellation, indexing, repository creation) to the React
//! frontend over typed IPC commands. Content writes, LFS and lock operations
//! are intentionally out of scope for this first pass.

use base64::Engine as _;
use open_pdm_core::{
    is_tracked, settings_file, FileEntry, FileMetadata, HandlerRegistry, Index, IndexStats, Mesh,
    Project, Revision, Settings, INTERNAL_DIR, PROJECT_EXTENSIONS, WORKTREE,
};
use serde::Serialize;
use std::path::PathBuf;

/// Flattened summary of an opened project.
#[derive(Serialize)]
struct ProjectInfo {
    root: String,
    bare: bool,
    branch: Option<String>,
    head: Option<Revision>,
    file_count: usize,
}

fn to_err<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

fn open(path: &str) -> Result<Project, String> {
    Project::open(path).map_err(to_err)
}

/// Handler registry, including the app's configured external converters
/// (see `$HOME/.openpdm/converters.json`). Enables `.sldprt`/`.sldasm` previews
/// in every project once a converter is installed.
fn registry() -> HandlerRegistry {
    HandlerRegistry::for_app()
}

/// App-level tracking settings, shared across every project.
fn app_settings() -> Settings {
    Settings::load(settings_file())
}

/// Where a project's own tracking settings live (`<root>/.openpdm/settings.json`).
///
/// The `.openpdm/` directory is machine-local and excluded from git, so these
/// settings never become project content.
fn project_settings_path(project: &Project) -> PathBuf {
    project.root().join(INTERNAL_DIR).join("settings.json")
}

/// Project-level tracking settings for `project`.
fn project_settings(project: &Project) -> Settings {
    Settings::load(project_settings_path(project))
}

/// Flatten a [`Project`] for transport to the UI.
fn project_info(project: &Project) -> ProjectInfo {
    let head = project.head_revision().ok();
    let file_count = project.files_at(None).map(|files| files.len()).unwrap_or(0);
    ProjectInfo {
        root: project.root().display().to_string(),
        bare: project.is_bare(),
        branch: project.branch(),
        head,
        file_count,
    }
}

#[tauri::command]
fn open_project(path: String) -> Result<ProjectInfo, String> {
    Ok(project_info(&open(&path)?))
}

/// Create a new git repository at `path` and return its summary.
///
/// Backs the "New" button: the UI picks a folder, then this initialises a
/// repository there so it can be browsed immediately.
#[tauri::command]
fn create_project(path: String) -> Result<ProjectInfo, String> {
    let project = Project::init(&path).map_err(to_err)?;
    Ok(project_info(&project))
}

#[tauri::command]
fn revisions(path: String, limit: usize) -> Result<Vec<Revision>, String> {
    open(&path)?.revisions(limit).map_err(to_err)
}

/// Commits that changed a single file, newest first.
///
/// Backs the per-file Revisions list in the detail panel. `limit` caps how far
/// back to walk, so opening a long-lived part does not read the whole history.
#[tauri::command]
fn file_revisions(path: String, file: String, limit: usize) -> Result<Vec<Revision>, String> {
    open(&path)?.file_revisions(&file, limit).map_err(to_err)
}

/// List the project files for a revision.
///
/// With `all` falsy this is the revision's *changed* files: the open
/// "Current" revision's working changes (staged and unstaged), or a commit's
/// diff against its first parent. With `all` truthy it lists every project
/// file: the whole working tree for the Current revision, or the whole tree
/// for a commit (its changed files marked with a `status`).
///
/// Only files OpenPDM recognises as project content (see
/// [`open_pdm_core::is_project_file`]) are returned; repository metadata and
/// generated files such as the `*.pdmdb` index are never shown or staged.
/// File types the user has deselected — app-wide or for this project — are
/// excluded too.
#[tauri::command]
fn files(path: String, rev: Option<String>, all: Option<bool>) -> Result<Vec<FileEntry>, String> {
    let project = open(&path)?;
    let app = app_settings();
    let project_level = project_settings(&project);
    let all = all.unwrap_or(false);
    let mut entries = match rev.as_deref() {
        Some(WORKTREE) if all => project.all_worktree_files(),
        Some(WORKTREE) => project.working_files(),
        Some(r) if all => project.annotated_files_at(r),
        Some(r) => project.changed_files_at(r),
        None => project.files_at(None),
    }
    .map_err(to_err)?;
    entries.retain(|entry| is_tracked(&entry.path, &app, &project_level));
    Ok(entries)
}

/// Stage a working change into the open "Current" revision.
#[tauri::command]
fn stage_file(path: String, file: String) -> Result<(), String> {
    open(&path)?.stage(&file).map_err(to_err)
}

/// Unstage a change from the open "Current" revision.
#[tauri::command]
fn unstage_file(path: String, file: String) -> Result<(), String> {
    open(&path)?.unstage(&file).map_err(to_err)
}

/// Discard a working change, restoring the file to its committed state.
#[tauri::command]
fn revert_file(path: String, file: String) -> Result<(), String> {
    open(&path)?.revert(&file).map_err(to_err)
}

/// Commit the staged changes of the open "Current" revision into history.
///
/// Backs the commit composer shown beneath the Revisions header. Returns the
/// new [`Revision`] so the UI can refresh history; rejects an empty message or
/// an empty staging area.
#[tauri::command]
fn commit(path: String, message: String) -> Result<Revision, String> {
    open(&path)?.commit(&message).map_err(to_err)
}

/// Restore a single file from a revision into the working tree.
///
/// Backs the detail panel's per-revision "check out" action. Only `file` is
/// touched; the restored content is staged so it becomes part of the open
/// "Current" revision.
#[tauri::command]
fn checkout_file(path: String, file: String, rev: String) -> Result<(), String> {
    open(&path)?.checkout_file(&rev, &file).map_err(to_err)
}

/// Remove a single file's version from history.
///
/// Backs the detail panel's per-revision "delete" action. Rewrites the commit
/// so it no longer changes `file` and rewrites the commits after it, so only
/// that version disappears; every other file is untouched. Commit oids change,
/// so the UI must refresh its history afterwards.
#[tauri::command]
fn delete_file_revision(path: String, file: String, rev: String) -> Result<(), String> {
    open(&path)?
        .delete_file_revision(&rev, &file)
        .map_err(to_err)
}

#[tauri::command]
fn metadata(path: String, file: String, rev: Option<String>) -> Result<FileMetadata, String> {
    let project = open(&path)?;
    let bytes = project.blob(rev.as_deref(), &file).map_err(to_err)?;
    let registry = registry();
    registry
        .resolve(&file)
        .metadata(&file, &bytes)
        .map_err(to_err)
}

/// Tessellate a file into a preview mesh.
///
/// Runs off the main thread: a configured external converter can take seconds
/// (up to the converter timeout), and a synchronous command would freeze the UI
/// for that whole time.
#[tauri::command]
async fn mesh(path: String, file: String, rev: Option<String>) -> Result<Option<Mesh>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let project = open(&path)?;
        let bytes = project.blob(rev.as_deref(), &file).map_err(to_err)?;
        let registry = registry();
        registry.resolve(&file).mesh(&file, &bytes).map_err(to_err)
    })
    .await
    .map_err(to_err)?
}

/// A raw shape payload sent to the viewer for client-side tessellation.
///
/// The bytes are base64-encoded because Tauri serialises command results as
/// JSON, where a `Vec<u8>` would inflate to one number per byte.
#[derive(Serialize)]
struct ShapePayloadDto {
    name: String,
    format: String,
    data: String,
}

/// Raw B-rep shape payloads (e.g. from a FreeCAD `.FCStd`) for the viewer to
/// tessellate with `occt-import-js`. Empty for formats that produce a mesh
/// directly (see [`mesh`]).
#[tauri::command]
fn shape_payloads(
    path: String,
    file: String,
    rev: Option<String>,
) -> Result<Vec<ShapePayloadDto>, String> {
    let project = open(&path)?;
    let bytes = project.blob(rev.as_deref(), &file).map_err(to_err)?;
    let registry = registry();
    let payloads = registry
        .resolve(&file)
        .shape_payloads(&file, &bytes)
        .map_err(to_err)?;
    Ok(payloads
        .into_iter()
        .map(|payload| ShapePayloadDto {
            name: payload.name,
            format: payload.format,
            data: base64::engine::general_purpose::STANDARD.encode(payload.data),
        })
        .collect())
}

/// Build (or refresh) the project's SQLite index.
///
/// Runs off the main thread: indexing walks history and can take a while.
/// File types the user has deselected — app-wide or for this project — are
/// left out of the index.
#[tauri::command]
async fn index_project(path: String, limit: usize) -> Result<IndexStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let project = open(&path)?;
        let app = app_settings();
        let project_level = project_settings(&project);
        let db = project.root().join(INTERNAL_DIR).join("index.pdmdb");
        let mut index = Index::open(&db).map_err(to_err)?;
        index
            .index_filtered(&project, limit, |file| {
                is_tracked(file, &app, &project_level)
            })
            .map_err(to_err)
    })
    .await
    .map_err(to_err)?
}

/// Read the project's existing index summary without re-indexing.
///
/// Returns `None` when the project has never been indexed. Backs the settings
/// dialog's Index tab, which reports the current index rather than starting a
/// run.
#[tauri::command]
async fn index_stats(path: String) -> Result<Option<IndexStats>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let project = open(&path)?;
        let db = project.root().join(INTERNAL_DIR).join("index.pdmdb");
        if !db.exists() {
            return Ok(None);
        }
        let index = Index::open(&db).map_err(to_err)?;
        index.stats().map(Some).map_err(to_err)
    })
    .await
    .map_err(to_err)?
}

/// The file extensions OpenPDM can track, for the settings dialog to list.
#[tauri::command]
fn trackable_extensions() -> Vec<String> {
    PROJECT_EXTENSIONS.iter().map(|ext| ext.to_string()).collect()
}

/// App-level tracking settings, shared across every project.
#[tauri::command]
fn get_app_settings() -> Settings {
    app_settings()
}

/// Persist app-level tracking settings.
#[tauri::command]
fn set_app_settings(settings: Settings) -> Result<(), String> {
    settings.save(settings_file()).map_err(to_err)
}

/// Tracking settings for a single project.
#[tauri::command]
fn get_project_settings(path: String) -> Result<Settings, String> {
    Ok(project_settings(&open(&path)?))
}

/// Persist tracking settings for a single project.
#[tauri::command]
fn set_project_settings(path: String, settings: Settings) -> Result<(), String> {
    let project = open(&path)?;
    settings
        .save(project_settings_path(&project))
        .map_err(to_err)
}

/// Total size, in bytes, of the app-level converted-artifact cache.
#[tauri::command]
fn cache_size() -> u64 {
    open_pdm_core::app::cache_size()
}

/// Empty the app-level converted-artifact cache. It rebuilds on demand, so this
/// only costs the time to reconvert files the next time they are previewed.
#[tauri::command]
fn clear_cache() -> Result<(), String> {
    open_pdm_core::app::clear_cache().map_err(to_err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Ensure the app-level converter directory (`$HOME/.openpdm`) exists, with
    // its `bin/` subdirectory and a starter `converters.json`, so the location
    // the user edits is discoverable without running a converter first.
    if let Err(error) = open_pdm_core::app::ensure() {
        eprintln!("warning: could not create the OpenPDM config directory: {error}");
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            create_project,
            revisions,
            file_revisions,
            files,
            stage_file,
            unstage_file,
            revert_file,
            commit,
            checkout_file,
            delete_file_revision,
            metadata,
            mesh,
            shape_payloads,
            index_project,
            index_stats,
            trackable_extensions,
            get_app_settings,
            set_app_settings,
            get_project_settings,
            set_project_settings,
            cache_size,
            clear_cache
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenPDM");
}
