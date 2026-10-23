import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type RefObject,
  type SetStateAction,
} from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CURRENT_REV,
  checkoutFile as checkoutFileAt,
  commitStaged,
  createProject,
  deleteFileRevision as deleteFileRevisionAt,
  getFileRevisions,
  getFiles,
  getMesh,
  getMetadata,
  getRevisions,
  getShapePayloads,
  indexProject,
  openProject,
  stageFile,
  unstageFile,
  type FileEntry,
  type FileMetadata,
  type IndexStats,
  type Mesh,
  type ProjectInfo,
  type Revision,
} from "@/lib/api";
import { getStored, setStored, type StoreValues } from "@/lib/localStore";
import { SECTION_DEFAULT_HEIGHT_PCT, SECTION_MIN_HEIGHT } from "./constants";

/** Row density used by the Files and Revisions list sections. */
export type ViewMode = StoreValues["filesView"] | StoreValues["revisionsView"];

/** UI colour theme. */
export type Theme = StoreValues["theme"];

export interface AppContextValue {
  // Project loaded into the workspace.
  projectPath: string;
  setProjectPath: Dispatch<SetStateAction<string>>;
  info: ProjectInfo | null;
  revisions: Revision[];
  selectedRev: string | null;
  isCurrent: boolean;
  files: FileEntry[];
  changeCount: number;
  selectedFile: FileEntry | null;
  metadata: FileMetadata | null;
  mesh: Mesh | null;
  /** Committed versions of the selected file, newest first; null while loading. */
  fileRevisions: Revision[] | null;
  /** Revision currently previewed for the selected file (commit oid or `CURRENT_REV`). */
  previewRev: string | null;
  /** True while a previous version is being checked out into the working tree. */
  checkingOut: boolean;
  /** True while a version is being deleted from history. */
  deletingRev: boolean;
  stats: IndexStats | null;
  error: string | null;
  busy: boolean;
  previewLoading: boolean;

  // Appearance and layout preferences, persisted across launches.
  theme: Theme;
  setTheme: Dispatch<SetStateAction<Theme>>;
  filesCollapsed: boolean;
  setFilesCollapsed: Dispatch<SetStateAction<boolean>>;
  revisionsCollapsed: boolean;
  setRevisionsCollapsed: Dispatch<SetStateAction<boolean>>;
  filesHeightPct: number;
  filesView: ViewMode;
  setFilesView: Dispatch<SetStateAction<ViewMode>>;
  /** Whether the Files section lists every project file, not just changes. */
  showAllFiles: boolean;
  /** Flip {@link showAllFiles} and reload the open revision's files. */
  toggleShowAllFiles: () => Promise<void>;
  revisionsView: ViewMode;
  setRevisionsView: Dispatch<SetStateAction<ViewMode>>;
  leftColumnRef: RefObject<HTMLDivElement | null>;

  // Project operations.
  loadPath: (path: string) => Promise<void>;
  openProjectPicker: () => Promise<void>;
  newProject: () => Promise<void>;
  buildIndex: () => Promise<void>;

  // Selection operations.
  pickRevision: (oid: string) => Promise<void>;
  pickFile: (file: FileEntry) => Promise<void>;
  /** Preview a specific version of the selected file. */
  pickFileRev: (oid: string) => Promise<void>;
  /** Restore the selected file from a revision into the working tree. */
  checkoutFile: (oid: string) => Promise<void>;
  /** Delete the selected file's version at a revision from history. */
  deleteFileRevision: (oid: string) => Promise<void>;
  toggleStaged: (file: FileEntry) => Promise<void>;

  // Commit composer for the open "Current" revision.
  commitMessage: string;
  setCommitMessage: Dispatch<SetStateAction<string>>;
  /** Number of changes staged into the open "Current" revision. */
  stagedCount: number;
  committing: boolean;
  commitCurrent: () => Promise<void>;

  // Left-column divider resize handlers.
  startResize: (e: ReactPointerEvent<HTMLDivElement>) => void;
  resize: (e: ReactPointerEvent<HTMLDivElement>) => void;
  endResize: (e: ReactPointerEvent<HTMLDivElement>) => void;
  resizeWithKeyboard: (e: ReactKeyboardEvent<HTMLDivElement>) => void;
}

const AppContext = createContext<AppContextValue | null>(null);

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

/**
 * Owns the desktop app's workspace state, layout preferences and operations.
 * Sections read whatever they need through {@link useApp} rather than receiving
 * props, keeping the component tree decoupled from the orchestrating state.
 */
export function AppProvider({ children }: { children: ReactNode }) {
  // Default to the demo project on a first run; later launches reopen whichever
  // project was last loaded.
  const [projectPath, setProjectPath] = useState(() => getStored("lastProjectPath", "/tmp/openpdm-demo"));
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [revisions, setRevisions] = useState<Revision[]>([]);
  const [selectedRev, setSelectedRev] = useState<string | null>(null);
  const [files, setFiles] = useState<FileEntry[]>([]);
  // Number of changed files in the open "Current" revision, kept even while a
  // committed revision is selected so the Current row can show a count.
  const [changeCount, setChangeCount] = useState(0);
  const [selectedFile, setSelectedFile] = useState<FileEntry | null>(null);
  const [metadata, setMetadata] = useState<FileMetadata | null>(null);
  const [mesh, setMesh] = useState<Mesh | null>(null);
  const [fileRevisions, setFileRevisions] = useState<Revision[] | null>(null);
  const [previewRev, setPreviewRev] = useState<string | null>(null);
  const [checkingOut, setCheckingOut] = useState(false);
  const [deletingRev, setDeletingRev] = useState(false);
  const [stats, setStats] = useState<IndexStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // True while a file's preview is being produced (e.g. B-rep tessellation).
  const [previewLoading, setPreviewLoading] = useState(false);
  // Draft commit message and in-flight flag for the Current revision composer.
  const [commitMessage, setCommitMessage] = useState("");
  const [committing, setCommitting] = useState(false);
  const [theme, setTheme] = useState<Theme>(() => getStored("theme", "light"));
  const [filesCollapsed, setFilesCollapsed] = useState(() => getStored("filesCollapsed", false));
  const [revisionsCollapsed, setRevisionsCollapsed] = useState(() => getStored("revisionsCollapsed", false));
  const [filesHeightPct, setFilesHeightPct] = useState(() => getStored("filesHeightPct", SECTION_DEFAULT_HEIGHT_PCT));
  const [filesView, setFilesView] = useState<ViewMode>(() => getStored("filesView", "detailed"));
  const [showAllFiles, setShowAllFiles] = useState(() => getStored("filesShowAll", false));
  const [revisionsView, setRevisionsView] = useState<ViewMode>(() => getStored("revisionsView", "detailed"));
  const leftColumnRef = useRef<HTMLDivElement>(null);
  // Monotonic id for the in-flight preview request, so a slow tessellation of a
  // previously selected file can never overwrite a newer selection.
  const previewRequestRef = useRef(0);
  const dragRef = useRef<{
    startY: number;
    startHeightPct: number;
    minPct: number;
    maxPct: number;
  } | null>(null);

  async function loadPath(path: string) {
    setError(null);
    setBusy(true);
    setStats(null);
    setCommitMessage("");
    try {
      setInfo(await openProject(path));
      // Remember the project that actually opened so the next launch can
      // restore it.
      setStored("lastProjectPath", path);
      const revs = await getRevisions(path, 50);
      setRevisions(revs);
      const rev = revs[0]?.oid ?? null;
      setSelectedRev(rev);
      setChangeCount((await getFiles(path, CURRENT_REV)).length);
      setFiles(await getFiles(path, rev, showAllFiles));
      clearFilePreview();
    } catch (e) {
      setError(String(e));
      setInfo(null);
      setRevisions([]);
      setFiles([]);
      setChangeCount(0);
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

  // Drop the selected file's preview state: metadata, mesh and version history.
  // Bumping the request id cancels any in-flight preview so a late response
  // cannot repopulate a file we have moved away from.
  function clearFilePreview() {
    setSelectedFile(null);
    setMetadata(null);
    setMesh(null);
    setFileRevisions(null);
    setPreviewRev(null);
    setPreviewLoading(false);
    previewRequestRef.current += 1;
  }

  async function loadFiles(path: string, rev: string | null, showAll: boolean) {
    setFiles(await getFiles(path, rev, showAll));
    clearFilePreview();
  }

  async function pickRevision(oid: string) {
    setSelectedRev(oid);
    setError(null);
    try {
      await loadFiles(projectPath, oid, showAllFiles);
    } catch (e) {
      setError(String(e));
    }
  }

  // Re-read the open revision's changed files, preserving the current file
  // selection (so staging does not deselect what you are looking at).
  async function refreshWorking() {
    const changes = await getFiles(projectPath, CURRENT_REV);
    setChangeCount(changes.length);
    if (selectedRev === CURRENT_REV) {
      setFiles(await getFiles(projectPath, CURRENT_REV, showAllFiles));
    }
  }

  // Flip the Files section between changes-only and every project file,
  // re-reading the open revision so the list reflects the new scope.
  async function toggleShowAllFiles() {
    const next = !showAllFiles;
    setShowAllFiles(next);
    if (!selectedRev) return;
    setError(null);
    try {
      await loadFiles(projectPath, selectedRev, next);
    } catch (e) {
      setError(String(e));
    }
  }

  // Add or remove a file from the open "Current" revision.
  async function toggleStaged(file: FileEntry) {
    setError(null);
    try {
      if (file.staged) {
        await unstageFile(projectPath, file.path);
      } else {
        await stageFile(projectPath, file.path);
      }
      await refreshWorking();
    } catch (e) {
      setError(String(e));
    }
  }

  // Commit the staged changes of the open "Current" revision into history, then
  // refresh the history list and the (now clean) working tree.
  async function commitCurrent() {
    if (stagedCount === 0) {
      setError("No staged changes to commit.");
      return;
    }
    const message = commitMessage.trim();
    if (!message) {
      setError("Enter a commit message.");
      return;
    }
    setError(null);
    setCommitting(true);
    try {
      await commitStaged(projectPath, message);
      setCommitMessage("");
      setRevisions(await getRevisions(projectPath, 50));
      await refreshWorking();
    } catch (e) {
      setError(String(e));
    } finally {
      setCommitting(false);
    }
  }

  // Load the metadata and preview mesh for one version of a file. `requestId`
  // is the caller's slot in the preview request sequence, so a slow response
  // for a file/version the user has since left is discarded.
  async function loadPreview(file: FileEntry, rev: string | null, requestId: number) {
    setMetadata(null);
    setMesh(null);
    setError(null);
    setPreviewLoading(true);
    try {
      const nextMetadata = await getMetadata(projectPath, file.path, rev);
      if (requestId !== previewRequestRef.current) return;
      setMetadata(nextMetadata);

      // Exact-geometry documents (FreeCAD .FCStd B-reps, STEP files) carry
      // shapes rather than triangles; the handler advertises a `tessellation`
      // codec and we tessellate the payloads in the webview with
      // occt-import-js. The module is imported dynamically so the wasm loads
      // only when needed.
      let nextMesh: Mesh | null;
      if (nextMetadata.attributes.tessellation) {
        const payloads = await getShapePayloads(projectPath, file.path, rev);
        const { tessellateShapes } = await import("@/lib/occt");
        nextMesh = await tessellateShapes(payloads);
      } else {
        nextMesh = await getMesh(projectPath, file.path, rev);
      }

      if (requestId !== previewRequestRef.current) return;
      setMesh(nextMesh);
    } catch (e) {
      if (requestId !== previewRequestRef.current) return;
      setError(String(e));
    } finally {
      if (requestId === previewRequestRef.current) setPreviewLoading(false);
    }
  }

  // Select a file: load its committed version history and preview the version
  // matching the open revision (the working tree for "Current", or the commit
  // being browsed).
  async function pickFile(file: FileEntry) {
    const requestId = (previewRequestRef.current += 1);
    setSelectedFile(file);
    setFileRevisions(null);
    setPreviewRev(selectedRev);
    setMetadata(null);
    setMesh(null);
    setError(null);
    setPreviewLoading(true);
    try {
      const revs = await getFileRevisions(projectPath, file.path);
      if (requestId !== previewRequestRef.current) return;
      setFileRevisions(revs);
      await loadPreview(file, selectedRev, requestId);
    } catch (e) {
      if (requestId !== previewRequestRef.current) return;
      setFileRevisions([]);
      setError(String(e));
      setPreviewLoading(false);
    }
  }

  // Preview a different committed version of the selected file.
  async function pickFileRev(oid: string) {
    const file = selectedFile;
    if (!file) return;
    setPreviewRev(oid);
    const requestId = (previewRequestRef.current += 1);
    await loadPreview(file, oid, requestId);
  }

  // Restore the selected file from `oid` into the working tree. Only that file
  // is touched; the restored content is staged, so it becomes a change in the
  // open "Current" revision.
  async function checkoutFile(oid: string) {
    const file = selectedFile;
    if (!file) return;
    setError(null);
    setCheckingOut(true);
    try {
      await checkoutFileAt(projectPath, file.path, oid);
      await refreshWorking();
      // The working tree now matches `oid`; show the restored content.
      setPreviewRev(oid);
      const requestId = (previewRequestRef.current += 1);
      await loadPreview(file, oid, requestId);
    } catch (e) {
      setError(String(e));
    } finally {
      setCheckingOut(false);
    }
  }

  // Delete the selected file's version at `oid` from history. The core rewrites
  // the commit chain, so the oids change: refresh the revision list, return to
  // the working tree, and rebuild the file's version history.
  async function deleteFileRevision(oid: string) {
    const file = selectedFile;
    if (!file) return;
    setError(null);
    setDeletingRev(true);
    try {
      await deleteFileRevisionAt(projectPath, file.path, oid);
      setRevisions(await getRevisions(projectPath, 50));
      setSelectedRev(CURRENT_REV);

      // One read of the whole working tree: the changed subset feeds the Files
      // panel, and it tells us whether the file still exists at all.
      const worktree = await getFiles(projectPath, CURRENT_REV, true);
      const changes = worktree.filter((f) => f.status != null || f.staged != null);
      setChangeCount(changes.length);
      setFiles(showAllFiles ? worktree : changes);

      if (!worktree.some((f) => f.path === file.path)) {
        // Deleting the file's only/earliest version removed it entirely.
        clearFilePreview();
      } else {
        setFileRevisions(await getFileRevisions(projectPath, file.path));
        setPreviewRev(CURRENT_REV);
        const requestId = (previewRequestRef.current += 1);
        await loadPreview(file, CURRENT_REV, requestId);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setDeletingRev(false);
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

  // Reflect the selected theme with shadcn's `.dark` class on <html>, and
  // remember the choice for the next launch.
  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    setStored("theme", theme);
  }, [theme]);

  // Persist the stacked-panel layout so heights, collapsed state and view modes
  // all survive reloads.
  useEffect(() => {
    setStored("filesHeightPct", filesHeightPct);
  }, [filesHeightPct]);

  useEffect(() => {
    setStored("filesCollapsed", filesCollapsed);
  }, [filesCollapsed]);

  useEffect(() => {
    setStored("revisionsCollapsed", revisionsCollapsed);
  }, [revisionsCollapsed]);

  useEffect(() => {
    setStored("filesView", filesView);
  }, [filesView]);

  useEffect(() => {
    setStored("filesShowAll", showAllFiles);
  }, [showAllFiles]);

  useEffect(() => {
    setStored("revisionsView", revisionsView);
  }, [revisionsView]);

  // Vertical drag handling for the divider between Files and Revisions. The
  // section height is kept as a percentage of the column so the split scales
  // with the window.
  function startResize(e: ReactPointerEvent<HTMLDivElement>) {
    const column = leftColumnRef.current;
    if (!column) return;
    const minPct = (SECTION_MIN_HEIGHT / column.getBoundingClientRect().height) * 100;
    dragRef.current = {
      startY: e.clientY,
      startHeightPct: filesHeightPct,
      minPct,
      maxPct: 100 - minPct,
    };
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  function resize(e: ReactPointerEvent<HTMLDivElement>) {
    const drag = dragRef.current;
    const column = leftColumnRef.current;
    if (!drag || !column) return;
    const deltaPct = ((e.clientY - drag.startY) / column.getBoundingClientRect().height) * 100;
    setFilesHeightPct(clamp(drag.startHeightPct + deltaPct, drag.minPct, drag.maxPct));
  }

  function endResize(e: ReactPointerEvent<HTMLDivElement>) {
    dragRef.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  }

  // Arrow keys nudge the divider for keyboard users.
  function resizeWithKeyboard(e: ReactKeyboardEvent<HTMLDivElement>) {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    const column = leftColumnRef.current;
    if (!column) return;
    const { height } = column.getBoundingClientRect();
    const minPct = (SECTION_MIN_HEIGHT / height) * 100;
    const step = ((e.key === "ArrowUp" ? -1 : 1) * 16 * 100) / height;
    setFilesHeightPct((pct) => clamp(pct + step, minPct, 100 - minPct));
  }

  const isCurrent = selectedRev === CURRENT_REV;
  // Staged changes only exist in the working tree; committed revisions have none.
  const stagedCount = isCurrent ? files.filter((file) => file.staged).length : 0;

  const value: AppContextValue = {
    projectPath,
    setProjectPath,
    info,
    revisions,
    selectedRev,
    isCurrent,
    files,
    changeCount,
    selectedFile,
    metadata,
    mesh,
    fileRevisions,
    previewRev,
    checkingOut,
    deletingRev,
    stats,
    error,
    busy,
    previewLoading,
    theme,
    setTheme,
    filesCollapsed,
    setFilesCollapsed,
    revisionsCollapsed,
    setRevisionsCollapsed,
    filesHeightPct,
    filesView,
    setFilesView,
    showAllFiles,
    toggleShowAllFiles,
    revisionsView,
    setRevisionsView,
    leftColumnRef,
    loadPath,
    openProjectPicker,
    newProject,
    buildIndex,
    pickRevision,
    pickFile,
    pickFileRev,
    checkoutFile,
    deleteFileRevision,
    toggleStaged,
    commitMessage,
    setCommitMessage,
    stagedCount,
    committing,
    commitCurrent,
    startResize,
    resize,
    endResize,
    resizeWithKeyboard,
  };

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

/** Access the app context. Must be called from within an {@link AppProvider}. */
export function useApp(): AppContextValue {
  const context = useContext(AppContext);
  if (!context) throw new Error("useApp must be used within an AppProvider");
  return context;
}
