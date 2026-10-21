import { Fragment, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  createProject,
  getFiles,
  getMesh,
  getMetadata,
  getRevisions,
  indexProject,
  openProject,
  type FileEntry,
  type FileMetadata,
  type IndexStats,
  type Mesh,
  type ProjectInfo,
  type Revision,
} from "./lib/api";
import Viewer from "./lib/Viewer";

function formatBytes(n: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${value.toFixed(1)} ${units[unit]}`;
}

function formatTime(seconds: number): string {
  const delta = Math.floor(Date.now() / 1000) - seconds;
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
}

function shortOid(oid: string): string {
  return oid.slice(0, 8);
}

export default function App() {
  const [projectPath, setProjectPath] = useState("/tmp/openpdm-demo");
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [revisions, setRevisions] = useState<Revision[]>([]);
  const [selectedRev, setSelectedRev] = useState<string | null>(null);
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [selectedFile, setSelectedFile] = useState<FileEntry | null>(null);
  const [metadata, setMetadata] = useState<FileMetadata | null>(null);
  const [mesh, setMesh] = useState<Mesh | null>(null);
  const [stats, setStats] = useState<IndexStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function loadPath(path: string) {
    setError(null);
    setBusy(true);
    setStats(null);
    try {
      setInfo(await openProject(path));
      const revs = await getRevisions(path, 50);
      setRevisions(revs);
      const rev = revs[0]?.oid ?? null;
      setSelectedRev(rev);
      setFiles(await getFiles(path, rev));
      setSelectedFile(null);
      setMetadata(null);
      setMesh(null);
    } catch (e) {
      setError(String(e));
      setInfo(null);
      setRevisions([]);
      setFiles([]);
    } finally {
      setBusy(false);
    }
  }

  // Open the native folder picker, then load the chosen project.
  async function openProjectPicker() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Open project folder",
    });
    if (typeof selected !== "string") return;
    setProjectPath(selected);
    await loadPath(selected);
  }

  // Choose a folder, initialise a git repository in it, then open the project.
  async function newProject() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose a folder for the new project",
    });
    if (typeof selected !== "string") return;
    setError(null);
    try {
      await createProject(selected);
    } catch (e) {
      setError(String(e));
      return;
    }
    setProjectPath(selected);
    await loadPath(selected);
  }

  async function loadFiles(path: string, rev: string | null) {
    setFiles(await getFiles(path, rev));
    setSelectedFile(null);
    setMetadata(null);
    setMesh(null);
  }

  async function pickRevision(oid: string) {
    setSelectedRev(oid);
    setError(null);
    try {
      await loadFiles(projectPath, oid);
    } catch (e) {
      setError(String(e));
    }
  }

  async function pickFile(file: FileEntry) {
    setSelectedFile(file);
    setMetadata(null);
    setMesh(null);
    setError(null);
    try {
      setMetadata(await getMetadata(projectPath, file.path, selectedRev));
      setMesh(await getMesh(projectPath, file.path, selectedRev));
    } catch (e) {
      setError(String(e));
    }
  }

  async function buildIndex() {
    setError(null);
    setBusy(true);
    try {
      setStats(await indexProject(projectPath, 200));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  // First pass: try to open the default demo path on launch. Intentionally
  // runs once; later loads are driven by the path box, Open and New.
  useEffect(() => {
    void loadPath(projectPath);
  }, []);

  return (
    <div className="app">
      <header>
        <div className="brand">OpenPDM</div>
        <input
          className="path"
          value={projectPath}
          placeholder="/path/to/project"
          onChange={(e) => setProjectPath(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && loadPath(projectPath)}
        />
        <button onClick={newProject} disabled={busy}>
          New
        </button>
        <button className="secondary" onClick={openProjectPicker} disabled={busy}>
          Open
        </button>
        <button className="secondary" onClick={buildIndex} disabled={busy || !info}>
          Index
        </button>
        {busy && <span className="muted">working…</span>}
      </header>

      {error && <div className="error">{error}</div>}

      <main>
        <aside className="column revisions">
          <h2>Revisions</h2>
          {revisions.length === 0 && (
            <p className="muted">No revisions. Open a repository above.</p>
          )}
          {revisions.map((revision) => (
            <button
              key={revision.oid}
              className={`row${revision.oid === selectedRev ? " active" : ""}`}
              onClick={() => pickRevision(revision.oid)}
            >
              <span className="mono">{revision.short}</span>
              <span className="summary">{revision.summary}</span>
              <span className="muted small">
                {revision.author} · {formatTime(revision.time)}
              </span>
            </button>
          ))}
        </aside>

        <section className="column files">
          <h2>
            Files
            {selectedRev && <span className="muted mono"> @ {shortOid(selectedRev)}</span>}
          </h2>
          {files.length === 0 && <p className="muted">No files.</p>}
          {files.map((file) => (
            <button
              key={file.oid + file.path}
              className={`row${selectedFile?.path === file.path ? " active" : ""}`}
              onClick={() => pickFile(file)}
            >
              <span className="filename">{file.path}</span>
              <span className="muted small">
                {formatBytes(file.size)}
                {file.is_binary ? " · binary" : ""}
              </span>
            </button>
          ))}
        </section>

        <section className="column detail">
          {selectedFile ? (
            <>
              <h2>{selectedFile.path}</h2>
              {metadata && (
                <dl className="meta">
                  <dt>Handler</dt>
                  <dd>{metadata.format}</dd>
                  {Object.entries(metadata.attributes).map(([key, value]) => (
                    <Fragment key={key}>
                      <dt>{key}</dt>
                      <dd className="mono">{value}</dd>
                    </Fragment>
                  ))}
                </dl>
              )}
              <div className="viewport">
                {mesh && mesh.indices.length > 0 ? (
                  <Viewer positions={mesh.positions} indices={mesh.indices} />
                ) : (
                  <p className="muted center">No preview available for this format.</p>
                )}
              </div>
            </>
          ) : (
            <>
              <h2>Detail</h2>
              <p className="muted">Select a file to inspect its metadata and preview.</p>
            </>
          )}

          {stats && (
            <div className="stats">
              <h3>Index</h3>
              <div>
                <span>Revisions</span>
                <b>{stats.revisions}</b>
              </div>
              <div>
                <span>File revisions</span>
                <b>{stats.file_revisions}</b>
              </div>
              <div>
                <span>Unique blobs</span>
                <b>{stats.unique_blobs}</b>
              </div>
              <div>
                <span>Unique bytes</span>
                <b>{formatBytes(stats.unique_bytes)}</b>
              </div>
              <div>
                <span>Referenced</span>
                <b>{formatBytes(stats.referenced_bytes)}</b>
              </div>
              <div>
                <span>Dedup saved</span>
                <b>{formatBytes(stats.dedup_saved_bytes)}</b>
              </div>
            </div>
          )}
        </section>
      </main>
    </div>
  );
}
