# Bundling third-party dependencies

Goal: remove the need for users to install third-party dependencies (git,
git-lfs, eDrawings, converters) themselves. Ship them with the app or install
them from within it, without relying on installer-specific logic.

## Decisions (agreed)

| Question | Decision |
| --- | --- |
| Scope | git + git-lfs (forward-looking), converter auto-config, eDrawings preview support |
| Platforms | **Windows first**; keep Linux/macOS on system git for now |
| eDrawings | **Download on first run** (with consent), not bundled in the installer |
| Windows installer | **Standardize on NSIS**; drop MSI |

## Key insight

With **download-on-first-run** for eDrawings and everything else shipped as
plain files, the design does **not need NSIS installer hooks**. One
installer-agnostic mechanism — a bundled tools tree resolved at runtime —
covers git, git-lfs and converters, and behaves identically in dev, NSIS,
deb/rpm and AppImage. Standardizing on NSIS is still worthwhile (simpler
upgrades, no stale MSI/sidecar mismatch), but it becomes a simplification
rather than a dependency.

## Architecture: an app-managed "tools" layer

Mirror the resolution logic that already exists in
`crates/core/src/convert.rs` (`CommandConverter::program_path`). New
resolution order for any external executable:

```
absolute path
  -> <install_dir>/resources/tools/**   (shipped with the app)
  -> ~/.openpdm/bin                     (user drop-in)
  -> PATH                               (last resort)
```

- The install dir comes from Tauri's `app.path().resource_dir()` — the real
  resources directory when installed, the target dir in dev.
- `~/.openpdm/bin` (see `crates/core/src/app.rs::bin_dir`) stays as the user's
  own drop-in location.

For git specifically, the helper must also set `GIT_EXEC_PATH` to the bundled
git's `libexec` and **prepend the bundled tools dir to the child `PATH`**, so
git finds its own subcommands *and* `git-lfs`.

> Keep `git2` for all local reads. It is statically linked (libgit2), so basic
> browsing already needs no external git. Only commit/remote/LFS use the
> bundled `git` binary.

## Phases

### Phase 1 — Foundation (installer-agnostic)

1. **New `crates/core/src/tools.rs`** — a `ToolPaths` struct:
   ```rust
   pub struct ToolPaths {
       /// Directory shipped with the app (Tauri resource dir in production).
       pub bundled: Option<PathBuf>,
       /// `~/.openpdm/bin` — user-installed tools.
       pub user_bin: PathBuf,
   }
   ```
   with `resolve(name) -> PathBuf` (absolute -> bundled -> user bin -> bare
   name for PATH) and a `command(name)` helper that sets `GIT_EXEC_PATH` and
   prepends the bundled dir to `PATH`.

2. **Refactor `CommandConverter`** (`crates/core/src/convert.rs`) to hold a
   `ToolPaths` instead of only `bin_dir`. Keep `with_bin_dir` as a thin wrapper
   so existing tests keep passing.

3. **Wire it in the host** (`apps/desktop/src-tauri/src/lib.rs`): replace the
   `HandlerRegistry::for_app()` call with a variant that takes the tools dir
   resolved from `resource_dir()`. Give the CLI (`crates/cli`) an optional
   `--tools-dir` flag.

4. **Ship the tools tree via `bundle.resources`** in
   `apps/desktop/src-tauri/tauri.conf.json` (not `externalBin` — resources
   handle a directory tree and avoid the known NSIS stale-sidecar bug):
   ```json
   "bundle": {
     "resources": { "../../tools/": "tools/" }
   }
   ```
   Layout:
   ```
   tools/
     git/**                            portable git (MinGit on Windows)
     git-lfs.exe                       (git-lfs-<target-triple> on other OSes)
     converters/edrawings-to-stl.exe
   ```

5. **CI** (`.github/workflows/release.yml`): a step that downloads a **pinned
   MinGit** (portable, no registry/PATH changes — ideal), verifies its
   checksum, extracts into `apps/desktop/src-tauri/resources/tools/`, and drops
   in `git-lfs.exe`. Bundle git's `COPYING` plus a written source offer
   alongside (GPLv2 compliance).

### Phase 2 — Converter auto-config

6. **Extend `app::ensure()`** (`crates/core/src/app.rs`, currently creates the
   dirs and writes an *empty* `converters.json`) to seed the registry with
   entries for bundled converters when a key is absent. Use a **bare name** (for
   example `edrawings-to-stl`) so resolution flows through the bundled tools
   dir and survives install-path changes:
   ```json
   {
     "converters": {
       "sldprt": { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] },
       "sldasm": { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] },
       "prt":    { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] },
       "asm":    { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] }
     }
   }
   ```

7. **Ship `edrawings-to-stl.exe`** in the tools tree.

   > **CI wrinkle:** building that project requires eDrawings installed on the
   > build machine to resolve the interop assembly (see
   > `examples/converters/edrawings-to-stl/README.md`). Options: pre-build and
   > vendor the exe, or use a self-hosted Windows runner. This blocks a clean
   > CI build and must be resolved.

### Phase 3 — git / git-lfs runtime wiring (forward-looking)

8. Add a `git` subprocess helper in core that goes through `ToolPaths`. Keep
   `git2` for local reads; use the bundled `git` only for commit/remote/LFS.
   For LFS, point the repo's `filter.lfs.*` config at the **absolute** bundled
   `git-lfs` path (more robust than relying on PATH at filter time).

### Phase 4 — eDrawings download-on-first-run

9. **Component status model** — `ComponentStatus { id, label, installed,
   detail }` exposed over IPC. Detect eDrawings via its COM CLSID / vendor
   registry key.

10. **A "Preview support" panel** in the settings UI listing git, git-lfs,
    the converter and eDrawings, each with status and an *Install* action for
    the missing ones.

11. **Bootstrap**: stream the official eDrawings installer to a temp file (via
    `reqwest` in the host) with **progress events** (`app.emit`), verify its
    signature/checksum, then launch it — silently if a supported flag exists,
    otherwise as a one-click guided install. Persist a marker under
    `~/.openpdm/`.

12. **Risks to resolve before promising this**:
    - No guaranteed stable direct-download URL for eDrawings; may need to pin a
      version or link to the vendor page.
    - Silent-install flags are undocumented (InstallShield-style `/s` needs
      verification).
    - Confirm redistribution/EULA terms.

### Phase 5 — Windows packaging

13. `.github/workflows/release.yml`: change the Windows matrix
    `bundles: msi,nsis` to `bundles: nsis`. `tauri.conf.json` keeps
    `targets: "all"`, overridden by the CLI arg.

14. Optionally add `bundle.windows.nsis.installerHooks` (`hooks.nsh`) later
    *only* if a genuine system-level step appears — not required by this
    design.

15. Third-party notices for git (GPLv2) and the converter.

## Already dependency-free (do not over-engineer)

- STEP / IGES / BREP / FCStd previews use `occt-import-js`, which is **bundled
  in the webview** (an npm dependency, no system install).
- Mesh formats (STL, OBJ, PLY, 3MF), COLLADA, glTF/GLB and FreeCAD metadata are
  parsed natively in Rust.
- Bundling git does not change any of the above.

## Open items to verify before coding

- **eDrawings installer**: direct URL + silent flags + redistribution terms.
  *(Highest risk.)*
- **MinGit pin + license files** to bundle; confirm Linux/macOS bundling is
  deferred (system git already present on both).
- **Converter CI build** requiring eDrawings on the runner.

## Suggested first step

De-risk the eDrawings installer question (Phase 4 items) before committing to
any code, since it is the only part with an unresolved external dependency.
