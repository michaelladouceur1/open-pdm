# OpenPDM

An open-source PDM (Product Data Management) app focused on CAD workflows,
built on **git + git-lfs** as the source of truth for file content.

This repository is a **first pass**: a working vertical slice from a real git
repository through to a live 3D preview.

![desktop app](docs/screenshot.png)

## What works today

- **Core (Rust)** — open any local git repository (no remote required), or
  create one, walk revision history, list files at any revision, read blobs.
- **Content-addressed index (SQLite)** — indexes revisions and files keyed by
  git blob oid, so identical content is stored once per revision _and_ across
  repositories. Fully rebuildable from git; never committed.
- **Pluggable CAD handlers** — format-specific behaviour sits behind a
  `CadHandler` trait. Built-ins: STL (binary + ASCII) and OBJ (mesh + metadata),
  with a generic fallback for everything else.
- **Desktop app (Tauri v2 + Svelte + three.js)** — browse revisions and files,
  inspect extracted metadata, and preview meshes in a 3D viewport.
- **Demo CLI (`opendm`)** — the same core exposed from the terminal.

Not yet implemented (by design, see the plan): writes, `git-lfs` transfer and
locking, STEP/IGES tessellation (`occt-import-js`), sidecar handlers, remote
providers, thumbnails.

## Architecture

```
┌─ Svelte + TypeScript + three.js (webview UI) ─────────┐
│  revisions · files · metadata · 3D viewer             │
└───────────────▲───────────────────────────────────────┘
      typed IPC │ (@tauri-apps/api invoke)
┌───────────────┴───────────────────────────────────────┐
│ Rust core (Tauri host)                                │
│  Project (git reads) · Index (SQLite) · Handlers      │
└───────────────────────────────────────────────────────┘
```

Design constraints baked in from the start:

- **Git is the source of truth**, the SQLite index is a rebuildable cache.
- **Content-addressing everywhere** (blob oid identity), so a future shared
  "vault" repo or cross-project BOM is a data-model change, not a rewrite.
- **Format-agnostic core**: nothing about SolidWorks/STEP etc. leaks into the
  core; it all lives in handlers (and, later, sidecars).

## Repository layout

```
crates/core/            Rust core: git, index, handlers, mesh
crates/cli/             `opendm` demo CLI
apps/desktop/           Tauri v2 app (Svelte + three.js frontend)
apps/desktop/src-tauri/ Tauri host (thin IPC wrapper over core)
```

## Prerequisites (Development)

- Rust (stable) — https://rustup.rs
- Node.js + pnpm
- Linux desktop build deps (Debian/Ubuntu):

  ```sh
  sudo apt update
  sudo apt install -y \
    build-essential pkg-config \
    libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

  `libdbus-1-dev` is required by Tauri's `tao` windowing layer; `pkg-config`
  is needed to locate the system libraries at build time.

- `git` (and `git-lfs` later, for LFS features)

## Build & run

### Core + CLI

```sh
cargo test --workspace          # run the test suite
cargo build --bin opendm

# Try it on any repo:
target/debug/opendm info        /path/to/repo
target/debug/opendm revisions   /path/to/repo -n 20
target/debug/opendm files       /path/to/repo
target/debug/opendm meta        /path/to/repo path/to/part.stl
target/debug/opendm mesh        /path/to/repo path/to/part.stl
target/debug/opendm index       /path/to/repo
target/debug/opendm handlers
```

### Desktop app

```sh
cd apps/desktop
pnpm install
pnpm tauri dev        # dev, with hot reload
pnpm tauri build      # release bundle
```

Use **Open** to pick the folder of an existing project, or **New** to pick a
folder and initialise a fresh git repository in it. The path box shows (and can
still be edited to) the current project; it defaults to `/tmp/opendm-demo`.

### Packaging a standalone binary

`tauri build` compiles the UI into the Rust binary (assets are embedded at
compile time via `generate_context!`), so the release executable needs no dev
server and no `node_modules`:

```sh
cd apps/desktop
pnpm tauri build
```

Outputs:

- **Standalone executable** — `src-tauri/target/release/open-pdm-app` (~15 MiB).
  Copy it anywhere and run it.
- **Installers** — `src-tauri/target/release/bundle/`:
  - `deb/OpenPDM_<ver>_amd64.deb` — `sudo dpkg -i <file>`, then run `open-pdm-app`
  - `rpm/OpenPDM-<ver>-1.x86_64.rpm`
  - `appimage/OpenPDM_<ver>_amd64.AppImage` — `chmod +x` and run (needs `libfuse2`,
    or use `--appimage-extract-and-run`)

The only runtime dependency is the OS webview (Linux: `libwebkit2gtk-4.1-0` +
`libgtk-3-0`; Windows: WebView2; macOS: system WKWebView). Bundles must be built
on their target OS — there is no Linux→Windows/macOS cross-compile.

## The handler seam

```rust
pub trait CadHandler: Send + Sync {
    fn name(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn metadata(&self, path: &str, bytes: &[u8]) -> Result<FileMetadata>;
    fn mesh(&self, path: &str, bytes: &[u8]) -> Result<Option<Mesh>>;
}
```

Adding a format means implementing this trait and registering it; the core and
UI do not change. Proprietary formats (SolidWorks, Inventor, …) are intended to
be handled later via a sidecar/JSON-RPC adapter rather than in-process parsing.

## License

MIT OR Apache-2.0
