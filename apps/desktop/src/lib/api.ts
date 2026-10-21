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

export interface FileEntry {
  path: string;
  oid: string;
  size: number;
  is_binary: boolean;
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

export const openProject = (path: string) =>
  invoke<ProjectInfo>("open_project", { path });

export const createProject = (path: string) =>
  invoke<ProjectInfo>("create_project", { path });

export const getRevisions = (path: string, limit: number) =>
  invoke<Revision[]>("revisions", { path, limit });

export const getFiles = (path: string, rev: string | null) =>
  invoke<FileEntry[]>("files", { path, rev });

export const getMetadata = (path: string, file: string, rev: string | null) =>
  invoke<FileMetadata>("metadata", { path, file, rev });

export const getMesh = (path: string, file: string, rev: string | null) =>
  invoke<Mesh | null>("mesh", { path, file, rev });

export const getShapePayloads = (path: string, file: string, rev: string | null) =>
  invoke<ShapePayload[]>("shape_payloads", { path, file, rev });

export const indexProject = (path: string, limit: number) =>
  invoke<IndexStats>("index_project", { path, limit });
