//! Tauri host for the OpenPDM desktop app.
//!
//! The host is deliberately thin: it exposes the read-only core (git history,
//! file metadata, tessellation, indexing) to the Svelte frontend over typed
//! IPC commands. Writes, LFS and lock operations are intentionally out of scope
//! for this first pass.

use open_pdm_core::{
    FileEntry, FileMetadata, HandlerRegistry, Index, IndexStats, Mesh, Project, Revision,
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

#[tauri::command]
fn open_project(path: String) -> Result<ProjectInfo, String> {
    let project = open(&path)?;
    let head = project.head_revision().ok();
    let file_count = project.files_at(None).map(|files| files.len()).unwrap_or(0);
    Ok(ProjectInfo {
        root: project.root().display().to_string(),
        bare: project.is_bare(),
        branch: project.branch(),
        head,
        file_count,
    })
}

#[tauri::command]
fn revisions(path: String, limit: usize) -> Result<Vec<Revision>, String> {
    open(&path)?.revisions(limit).map_err(to_err)
}

#[tauri::command]
fn files(path: String, rev: Option<String>) -> Result<Vec<FileEntry>, String> {
    open(&path)?.files_at(rev.as_deref()).map_err(to_err)
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

#[tauri::command]
fn index_project(path: String, limit: usize) -> Result<IndexStats, String> {
    let project = open(&path)?;
    let db = project.root().join(".opendm").join("index.pdmdb");
    let mut index = Index::open(&db).map_err(to_err)?;
    index.index(&project, limit).map_err(to_err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            open_project,
            revisions,
            files,
            metadata,
            mesh,
            index_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenPDM");
}
