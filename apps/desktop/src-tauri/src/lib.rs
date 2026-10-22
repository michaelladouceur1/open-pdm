//! Tauri host for the OpenPDM desktop app.
//!
//! The host is deliberately thin: it exposes the core (git history, file
//! metadata, tessellation, indexing, repository creation) to the React
//! frontend over typed IPC commands. Content writes, LFS and lock operations
//! are intentionally out of scope for this first pass.

use base64::Engine as _;
use open_pdm_core::{
    is_project_file, FileEntry, FileMetadata, HandlerRegistry, Index, IndexStats, Mesh, Project,
    Revision, WORKTREE,
};
use serde::Serialize;

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

/// List the project files at a revision, or the changed files of the open
/// "Current" revision when `rev` is [`WORKTREE`].
///
/// Only files OpenPDM recognises as project content (see
/// [`open_pdm_core::is_project_file`]) are returned; repository metadata and
/// generated files such as the `*.pdmdb` index are never shown or staged.
#[tauri::command]
fn files(path: String, rev: Option<String>) -> Result<Vec<FileEntry>, String> {
    let project = open(&path)?;
    let mut entries = if rev.as_deref() == Some(WORKTREE) {
        project.working_files()
    } else {
        project.files_at(rev.as_deref())
    }
    .map_err(to_err)?;
    entries.retain(|entry| is_project_file(&entry.path));
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

#[tauri::command]
fn metadata(path: String, file: String, rev: Option<String>) -> Result<FileMetadata, String> {
    let project = open(&path)?;
    let bytes = project.blob(rev.as_deref(), &file).map_err(to_err)?;
    let registry = HandlerRegistry::builtin();
    registry
        .resolve(&file)
        .metadata(&file, &bytes)
        .map_err(to_err)
}

#[tauri::command]
fn mesh(path: String, file: String, rev: Option<String>) -> Result<Option<Mesh>, String> {
    let project = open(&path)?;
    let bytes = project.blob(rev.as_deref(), &file).map_err(to_err)?;
    let registry = HandlerRegistry::builtin();
    registry.resolve(&file).mesh(&file, &bytes).map_err(to_err)
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
    let registry = HandlerRegistry::builtin();
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

#[tauri::command]
fn index_project(path: String, limit: usize) -> Result<IndexStats, String> {
    let project = open(&path)?;
    let db = project.root().join(".openpdm").join("index.pdmdb");
    let mut index = Index::open(&db).map_err(to_err)?;
    index.index(&project, limit).map_err(to_err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            create_project,
            revisions,
            files,
            stage_file,
            unstage_file,
            metadata,
            mesh,
            shape_payloads,
            index_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenPDM");
}
