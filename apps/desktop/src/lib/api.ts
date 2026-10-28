import { invoke } from "@tauri-apps/api/core";

// These mirror the `Serialize` types in open-pdm-core / the Tauri commands.

export interface Revision {
  oid: string;
  short: string;
  summary: string;
  author: string;
  email: string;
  time: number;
  parents: number;
}

// Sentinel revision selecting the open, uncommitted "Current" revision. It must
// match `WORKTREE` in open-pdm-core; passed anywhere a revision oid is accepted.
export const CURRENT_REV = ":worktree:";

export interface FileEntry {
  path: string;
  oid: string;
  size: number;
  is_binary: boolean;
  /** Whether the change is staged into the Current revision. Absent for commits. */
  staged?: boolean;
  /** Change kind for the Current revision: new, modified, deleted, etc. */
  status?: string;
}

export interface ProjectInfo {
  root: string;
  bare: boolean;
  branch: string | null;
  head: Revision | null;
  file_count: number;
}

export interface FileMetadata {
  format: string;
  attributes: Record<string, string>;
}

export interface Mesh {
  positions: number[];
  indices: number[];
}

// A raw shape payload (e.g. a FreeCAD B-rep) for client-side tessellation.
// `data` is base64-encoded; `format` names the codec the viewer should use.
export interface ShapePayload {
  name: string;
  format: string;
  data: string;
}

export interface IndexStats {
  repositories: number;
  revisions: number;
  file_revisions: number;
  unique_blobs: number;
  unique_bytes: number;
  referenced_bytes: number;
  dedup_saved_bytes: number;
}

// User-configurable file-tracking settings, mirrored from the core `Settings`.
// Only the extensions listed in `ignored_extensions` are excluded; everything
// OpenPDM recognises is tracked by default.
export interface Settings {
  ignored_extensions: string[];
}

export const openProject = (path: string) =>
  invoke<ProjectInfo>("open_project", { path });

export const createProject = (path: string) =>
  invoke<ProjectInfo>("create_project", { path });

export const getRevisions = (path: string, limit: number) =>
  invoke<Revision[]>("revisions", { path, limit });

/** Commits that changed a single file, newest first. */
export const getFileRevisions = (path: string, file: string, limit = 100) =>
  invoke<Revision[]>("file_revisions", { path, file, limit });

/** Restore a single file from a revision into the working tree. */
export const checkoutFile = (path: string, file: string, rev: string) =>
  invoke<void>("checkout_file", { path, file, rev });

/** Remove a single file's version from history (rewrites the commit chain). */
export const deleteFileRevision = (path: string, file: string, rev: string) =>
  invoke<void>("delete_file_revision", { path, file, rev });

export const getFiles = (path: string, rev: string | null, all = false) =>
  invoke<FileEntry[]>("files", { path, rev, all });

export const stageFile = (path: string, file: string) =>
  invoke<void>("stage_file", { path, file });

export const unstageFile = (path: string, file: string) =>
  invoke<void>("unstage_file", { path, file });

/** Discard a working change, restoring the file to its committed state. */
export const revertFile = (path: string, file: string) =>
  invoke<void>("revert_file", { path, file });

/** Commit the staged changes of the open "Current" revision into history. */
export const commitStaged = (path: string, message: string) =>
  invoke<Revision>("commit", { path, message });

export const getMetadata = (path: string, file: string, rev: string | null) =>
  invoke<FileMetadata>("metadata", { path, file, rev });

export const getMesh = (path: string, file: string, rev: string | null) =>
  invoke<Mesh | null>("mesh", { path, file, rev });

export const getShapePayloads = (path: string, file: string, rev: string | null) =>
  invoke<ShapePayload[]>("shape_payloads", { path, file, rev });

export const indexProject = (path: string, limit: number) =>
  invoke<IndexStats>("index_project", { path, limit });

/** The project's existing index summary, or null when it has never been indexed. */
export const getIndexStats = (path: string) =>
  invoke<IndexStats | null>("index_stats", { path });

/** File extensions OpenPDM can track, for the settings dialog to list. */
export const getTrackableExtensions = () =>
  invoke<string[]>("trackable_extensions");

/** App-level tracking settings, shared across every project. */
export const getAppSettings = () => invoke<Settings>("get_app_settings");

export const setAppSettings = (settings: Settings) =>
  invoke<void>("set_app_settings", { settings });

/** Tracking settings for a single project. */
export const getProjectSettings = (path: string) =>
  invoke<Settings>("get_project_settings", { path });

export const setProjectSettings = (path: string, settings: Settings) =>
  invoke<void>("set_project_settings", { path, settings });
