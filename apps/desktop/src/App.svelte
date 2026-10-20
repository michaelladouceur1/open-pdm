<script lang="ts">
  import { onMount } from "svelte";
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
  import Viewer from "./lib/Viewer.svelte";

  let projectPath = $state("/tmp/openpdm-demo");
  let info = $state<ProjectInfo | null>(null);
  let revisions = $state<Revision[]>([]);
  let selectedRev = $state<string | null>(null);
  let files = $state<FileEntry[]>([]);
  let selectedFile = $state<FileEntry | null>(null);
  let metadata = $state<FileMetadata | null>(null);
  let mesh = $state<Mesh | null>(null);
  let stats = $state<IndexStats | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  async function load() {
    error = null;
    busy = true;
    stats = null;
    try {
      info = await openProject(projectPath);
      revisions = await getRevisions(projectPath, 50);
      selectedRev = revisions[0]?.oid ?? null;
      await loadFiles();
    } catch (e) {
      error = String(e);
      info = null;
      revisions = [];
      files = [];
    } finally {
      busy = false;
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
    projectPath = selected;
    await load();
  }

  // Choose a folder, initialise a git repository in it, then open the project.
  async function newProject() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose a folder for the new project",
    });
    if (typeof selected !== "string") return;
    error = null;
    try {
      await createProject(selected);
    } catch (e) {
      error = String(e);
      return;
    }
    projectPath = selected;
    await load();
  }

  async function loadFiles() {
    files = await getFiles(projectPath, selectedRev);
    selectedFile = null;
    metadata = null;
    mesh = null;
  }

  async function pickRevision(oid: string) {
    selectedRev = oid;
    error = null;
    try {
      await loadFiles();
    } catch (e) {
      error = String(e);
    }
  }

  async function pickFile(file: FileEntry) {
    selectedFile = file;
    metadata = null;
    mesh = null;
    error = null;
    try {
      metadata = await getMetadata(projectPath, file.path, selectedRev);
      mesh = await getMesh(projectPath, file.path, selectedRev);
    } catch (e) {
      error = String(e);
    }
  }

  async function buildIndex() {
    error = null;
    busy = true;
    try {
      stats = await indexProject(projectPath, 200);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

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

  // First pass: try to open the default demo path on launch.
  onMount(load);
</script>

<div class="app">
  <header>
    <div class="brand">OpenPDM</div>
    <input
      class="path"
      bind:value={projectPath}
      placeholder="/path/to/project"
      onkeydown={(e) => e.key === "Enter" && load()}
    />
    <button onclick={newProject} disabled={busy}>New</button>
    <button class="secondary" onclick={openProjectPicker} disabled={busy}>Open</button>
    <button class="secondary" onclick={buildIndex} disabled={busy || !info}>Index</button>
    {#if busy}<span class="muted">working…</span>{/if}
  </header>

  {#if error}
    <div class="error">{error}</div>
  {/if}

  <main>
    <aside class="column revisions">
      <h2>Revisions</h2>
      {#if revisions.length === 0}
        <p class="muted">No revisions. Open a repository above.</p>
      {/if}
      {#each revisions as revision (revision.oid)}
        <button
          class="row"
          class:active={revision.oid === selectedRev}
          onclick={() => pickRevision(revision.oid)}
        >
          <span class="mono">{revision.short}</span>
          <span class="summary">{revision.summary}</span>
          <span class="muted small">{revision.author} · {formatTime(revision.time)}</span>
        </button>
      {/each}
    </aside>

    <section class="column files">
      <h2>
        Files
        {#if selectedRev}<span class="muted mono">@ {shortOid(selectedRev)}</span>{/if}
      </h2>
      {#if files.length === 0}
        <p class="muted">No files.</p>
      {/if}
      {#each files as file (file.oid + file.path)}
        <button
          class="row"
          class:active={selectedFile?.path === file.path}
          onclick={() => pickFile(file)}
        >
          <span class="filename">{file.path}</span>
          <span class="muted small">
            {formatBytes(file.size)}{file.is_binary ? " · binary" : ""}
          </span>
        </button>
      {/each}
    </section>

    <section class="column detail">
      {#if selectedFile}
        <h2>{selectedFile.path}</h2>
        {#if metadata}
          <dl class="meta">
            <dt>Handler</dt>
            <dd>{metadata.format}</dd>
            {#each Object.entries(metadata.attributes) as [key, value] (key)}
              <dt>{key}</dt>
              <dd class="mono">{value}</dd>
            {/each}
          </dl>
        {/if}
        <div class="viewport">
          {#if mesh && mesh.indices.length > 0}
            <Viewer positions={mesh.positions} indices={mesh.indices} />
          {:else}
            <p class="muted center">No preview available for this format.</p>
          {/if}
        </div>
      {:else}
        <h2>Detail</h2>
        <p class="muted">Select a file to inspect its metadata and preview.</p>
      {/if}

      {#if stats}
        <div class="stats">
          <h3>Index</h3>
          <div><span>Revisions</span><b>{stats.revisions}</b></div>
          <div><span>File revisions</span><b>{stats.file_revisions}</b></div>
          <div><span>Unique blobs</span><b>{stats.unique_blobs}</b></div>
          <div><span>Unique bytes</span><b>{formatBytes(stats.unique_bytes)}</b></div>
          <div><span>Referenced</span><b>{formatBytes(stats.referenced_bytes)}</b></div>
          <div><span>Dedup saved</span><b>{formatBytes(stats.dedup_saved_bytes)}</b></div>
        </div>
      {/if}
    </section>
  </main>
</div>

<style>
  :global(html) {
    background: #101217;
    color-scheme: dark;
  }

  :global(body) {
    margin: 0;
    min-height: 100vh;
    font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
    background: #101217;
    color: #e6e8ee;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #101217;
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    background: #181b22;
    border-bottom: 1px solid #262a34;
  }

  .brand {
    font-weight: 700;
    letter-spacing: 0.5px;
    color: #7fb2ff;
  }

  .path {
    flex: 1;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid #2c313c;
    background: #10131a;
    color: inherit;
    font-family: ui-monospace, monospace;
  }

  button {
    padding: 8px 14px;
    border-radius: 6px;
    border: 1px solid #3a6ea5;
    background: #2f6fb5;
    color: #fff;
    cursor: pointer;
    font-weight: 600;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  button.secondary {
    background: transparent;
    border-color: #2c313c;
    color: #cdd3e0;
  }

  .error {
    padding: 10px 16px;
    background: #3a1d22;
    color: #ff9aa6;
    border-bottom: 1px solid #5a2a32;
    font-family: ui-monospace, monospace;
    font-size: 13px;
  }

  main {
    flex: 1;
    display: grid;
    grid-template-columns: 260px 300px 1fr;
    min-height: 0;
  }

  .column {
    min-height: 0;
    overflow-y: auto;
    padding: 12px;
    border-right: 1px solid #262a34;
  }
  .detail {
    border-right: none;
  }

  h2 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: #8b93a3;
    margin: 4px 0 10px;
  }
  h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: #8b93a3;
    margin: 16px 0 8px;
  }

  .row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    margin-bottom: 6px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: #171a21;
    color: inherit;
    font-weight: 500;
    cursor: pointer;
  }
  .row:hover {
    background: #1d2129;
  }
  .row.active {
    border-color: #3a6ea5;
    background: #1b2432;
  }

  .summary,
  .filename {
    font-size: 13px;
    color: #e6e8ee;
  }

  .muted {
    color: #8b93a3;
  }
  .small {
    font-size: 11px;
  }
  .mono {
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .center {
    text-align: center;
    margin-top: 40%;
  }

  .meta {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 16px;
    margin: 0 0 12px;
    font-size: 13px;
  }
  .meta dt {
    color: #8b93a3;
  }
  .meta dd {
    margin: 0;
  }

  .viewport {
    height: 360px;
    border: 1px solid #262a34;
    border-radius: 8px;
    overflow: hidden;
  }

  .stats div {
    display: flex;
    justify-content: space-between;
    padding: 4px 0;
    border-bottom: 1px solid #1e222b;
    font-size: 13px;
  }
  .stats span {
    color: #8b93a3;
  }
</style>
