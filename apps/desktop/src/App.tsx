import {
  Fragment,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ChevronRightIcon,
  CircleAlertIcon,
  DatabaseIcon,
  FolderOpenIcon,
  FolderPlusIcon,
  MoonIcon,
  Rows2Icon,
  Rows4Icon,
  SunIcon,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
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
} from "@/lib/api";
import { getStored, setStored, type StoreValues } from "@/lib/localStore";
import { cn } from "@/lib/utils";
import Viewer from "@/lib/Viewer";

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

// Smallest height either stacked section may be dragged to, in pixels.
const SECTION_MIN_HEIGHT = 96;

// Default share of the left column given to the Files section, as a percentage.
const SECTION_DEFAULT_HEIGHT_PCT = 40;

type ViewMode = StoreValues["filesView"] | StoreValues["revisionsView"];

type Theme = StoreValues["theme"];

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

// A selectable list row, used for both revisions and files. Rows stack their
// contents in "detailed" mode and collapse to a single line in "compact" mode.
function ListRow({
  active,
  onClick,
  compact = false,
  children,
}: {
  active: boolean;
  onClick: () => void;
  compact?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full rounded-lg border text-left transition-colors",
        compact ? "flex-row items-center gap-2 px-2.5 py-1" : "flex-col items-start gap-0.5 px-2.5 py-2",
        active ? "border-primary/40 bg-accent text-accent-foreground" : "border-transparent bg-muted/40 hover:bg-muted",
      )}
    >
      {children}
    </button>
  );
}

function StatRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between border-b py-1 text-[13px] last:border-0">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{value}</span>
    </div>
  );
}

// Collapsible header for a stacked section. The whole row toggles the section.
function SectionHeader({
  title,
  collapsed,
  onToggle,
  children,
}: {
  title: string;
  collapsed: boolean;
  onToggle: () => void;
  children?: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-expanded={!collapsed}
      className="flex w-full shrink-0 items-center gap-2 px-3 pt-3 pb-2 text-left"
    >
      <ChevronRightIcon
        className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", !collapsed && "rotate-90")}
      />
      <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{title}</h2>
      {children}
    </button>
  );
}

// Container for per-section actions, rendered just below the section header.
function SectionActions({ children }: { children: ReactNode }) {
  return <div className="flex items-center gap-1 px-3 pb-2 text-muted-foreground">{children}</div>;
}

// Icon button that flips a section between detailed and compact rows.
function ViewModeButton({ mode, onToggle }: { mode: ViewMode; onToggle: () => void }) {
  const detailed = mode === "detailed";
  const label = detailed ? "Switch to compact view" : "Switch to detailed view";
  return (
    <Tooltip>
      <TooltipTrigger
        className={cn(buttonVariants({ variant: "ghost", size: "icon-xs" }))}
        onClick={onToggle}
        aria-label={label}
      >
        {detailed ? <Rows2Icon /> : <Rows4Icon />}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
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
  const [theme, setTheme] = useState<Theme>(() => getStored("theme", "light"));
  const [filesCollapsed, setFilesCollapsed] = useState(() => getStored("filesCollapsed", false));
  const [revisionsCollapsed, setRevisionsCollapsed] = useState(() => getStored("revisionsCollapsed", false));
  const [filesHeightPct, setFilesHeightPct] = useState(() => getStored("filesHeightPct", SECTION_DEFAULT_HEIGHT_PCT));
  const [filesView, setFilesView] = useState<ViewMode>(() => getStored("filesView", "detailed"));
  const [revisionsView, setRevisionsView] = useState<ViewMode>(() => getStored("revisionsView", "detailed"));
  const leftColumnRef = useRef<HTMLDivElement>(null);
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

  const themeLabel = theme === "light" ? "Switch to dark theme" : "Switch to light theme";

  return (
    <TooltipProvider>
      <div className="flex h-screen flex-col bg-background text-foreground">
        <header className="flex items-center gap-2 border-b bg-card px-3 py-2">
          <div className="px-1 text-sm font-semibold tracking-wide text-primary">OpenPDM</div>
          <Separator orientation="vertical" className="mx-0.5 h-5" />
          <Input
            className="h-8 min-w-0 flex-1 font-mono"
            value={projectPath}
            placeholder="/path/to/project"
            onChange={(e) => setProjectPath(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && loadPath(projectPath)}
          />
          <Button onClick={newProject} disabled={busy}>
            <FolderPlusIcon data-icon="inline-start" />
            New
          </Button>
          <Button variant="outline" onClick={openProjectPicker} disabled={busy}>
            <FolderOpenIcon data-icon="inline-start" />
            Open
          </Button>
          <Button variant="outline" onClick={buildIndex} disabled={busy || !info}>
            <DatabaseIcon data-icon="inline-start" />
            Index
          </Button>
          <Tooltip>
            <TooltipTrigger
              className={cn(buttonVariants({ variant: "outline", size: "icon" }))}
              onClick={() => setTheme((t) => (t === "light" ? "dark" : "light"))}
              aria-label={themeLabel}
            >
              {theme === "light" ? <SunIcon /> : <MoonIcon />}
            </TooltipTrigger>
            <TooltipContent>{themeLabel}</TooltipContent>
          </Tooltip>
          {busy && (
            <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Spinner className="size-3.5" />
              working…
            </span>
          )}
        </header>

        {error && (
          <div className="border-b px-3 py-2">
            <Alert variant="destructive">
              <CircleAlertIcon />
              <AlertTitle>Something went wrong</AlertTitle>
              <AlertDescription className="font-mono text-xs break-all">{error}</AlertDescription>
            </Alert>
          </div>
        )}

        <main className="grid min-h-0 flex-1 grid-cols-[minmax(240px,320px)_1fr]">
          <aside ref={leftColumnRef} className="flex min-h-0 flex-col border-r">
            <section
              className={cn("flex min-h-0 flex-col", revisionsCollapsed ? "flex-1" : "shrink-0")}
              style={
                filesCollapsed || revisionsCollapsed
                  ? undefined
                  : {
                      height: `${filesHeightPct}%`,
                      maxHeight: `calc(100% - ${SECTION_MIN_HEIGHT}px)`,
                    }
              }
            >
              <SectionHeader title="Files" collapsed={filesCollapsed} onToggle={() => setFilesCollapsed((c) => !c)}>
                {selectedRev && (
                  <Badge variant="outline" className="font-mono">
                    @ {shortOid(selectedRev)}
                  </Badge>
                )}
              </SectionHeader>
              <SectionActions>
                <ViewModeButton
                  mode={filesView}
                  onToggle={() => setFilesView((m) => (m === "detailed" ? "compact" : "detailed"))}
                />
              </SectionActions>
              {!filesCollapsed && (
                <ScrollArea className="min-h-0 flex-1">
                  <div className="flex flex-col gap-1.5 px-3 pb-3">
                    {files.length === 0 && <p className="text-xs text-muted-foreground">No files.</p>}
                    {files.map((file) =>
                      filesView === "compact" ? (
                        <ListRow
                          key={file.oid + file.path}
                          compact
                          active={selectedFile?.path === file.path}
                          onClick={() => pickFile(file)}
                        >
                          <span className="min-w-0 flex-1 truncate text-[13px]">{file.path}</span>
                          <span className="shrink-0 text-[11px] text-muted-foreground">{formatBytes(file.size)}</span>
                        </ListRow>
                      ) : (
                        <ListRow
                          key={file.oid + file.path}
                          active={selectedFile?.path === file.path}
                          onClick={() => pickFile(file)}
                        >
                          <span className="text-[13px] break-all">{file.path}</span>
                          <span className="text-[11px] text-muted-foreground">
                            {formatBytes(file.size)}
                            {file.is_binary ? " · binary" : ""}
                          </span>
                        </ListRow>
                      ),
                    )}
                  </div>
                </ScrollArea>
              )}
            </section>

            {!filesCollapsed && !revisionsCollapsed && (
              <div
                role="separator"
                aria-orientation="horizontal"
                aria-label="Resize Files and Revisions"
                aria-valuenow={Math.round(filesHeightPct)}
                tabIndex={0}
                onPointerDown={startResize}
                onPointerMove={resize}
                onPointerUp={endResize}
                onKeyDown={resizeWithKeyboard}
                className="group flex h-2 shrink-0 cursor-row-resize touch-none items-center justify-center border-y bg-muted/40 select-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
              >
                <div className="h-0.5 w-8 rounded-full bg-border transition-colors group-hover:bg-primary/50" />
              </div>
            )}

            <section className={cn("flex min-h-0 flex-col", revisionsCollapsed ? "shrink-0" : "flex-1")}>
              <SectionHeader
                title="Revisions"
                collapsed={revisionsCollapsed}
                onToggle={() => setRevisionsCollapsed((c) => !c)}
              >
                {revisions.length > 0 && <Badge variant="secondary">{revisions.length}</Badge>}
              </SectionHeader>
              <SectionActions>
                <ViewModeButton
                  mode={revisionsView}
                  onToggle={() => setRevisionsView((m) => (m === "detailed" ? "compact" : "detailed"))}
                />
              </SectionActions>
              {!revisionsCollapsed && (
                <ScrollArea className="min-h-0 flex-1">
                  <div className="flex flex-col gap-1.5 px-3 pb-3">
                    {revisions.length === 0 && (
                      <p className="text-xs text-muted-foreground">No revisions. Open a repository above.</p>
                    )}
                    {revisions.map((revision) =>
                      revisionsView === "compact" ? (
                        <ListRow
                          key={revision.oid}
                          compact
                          active={revision.oid === selectedRev}
                          onClick={() => pickRevision(revision.oid)}
                        >
                          <span className="shrink-0 font-mono text-xs text-muted-foreground">{revision.short}</span>
                          <span className="min-w-0 flex-1 truncate text-[13px]">{revision.summary}</span>
                          <span className="shrink-0 text-[11px] text-muted-foreground">
                            {formatTime(revision.time)}
                          </span>
                        </ListRow>
                      ) : (
                        <ListRow
                          key={revision.oid}
                          active={revision.oid === selectedRev}
                          onClick={() => pickRevision(revision.oid)}
                        >
                          <span className="font-mono text-xs text-muted-foreground">{revision.short}</span>
                          <span className="line-clamp-2 text-[13px]">{revision.summary}</span>
                          <span className="text-[11px] text-muted-foreground">
                            {revision.author} · {formatTime(revision.time)}
                          </span>
                        </ListRow>
                      ),
                    )}
                  </div>
                </ScrollArea>
              )}
            </section>
          </aside>

          <section className="flex min-h-0 flex-col gap-3 overflow-y-auto p-3">
            {selectedFile ? (
              <>
                <div className="flex flex-col gap-1.5">
                  <h1 className="font-mono text-sm break-all">{selectedFile.path}</h1>
                  <div className="flex items-center gap-1.5">
                    <Badge variant="secondary">{formatBytes(selectedFile.size)}</Badge>
                    {selectedFile.is_binary && <Badge variant="outline">binary</Badge>}
                  </div>
                </div>

                {metadata && (
                  <Card size="sm">
                    <CardHeader>
                      <CardTitle>Metadata</CardTitle>
                    </CardHeader>
                    <CardContent>
                      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13px]">
                        <dt className="text-muted-foreground">Handler</dt>
                        <dd>{metadata.format}</dd>
                        {Object.entries(metadata.attributes).map(([key, value]) => (
                          <Fragment key={key}>
                            <dt className="text-muted-foreground">{key}</dt>
                            <dd className="font-mono text-xs break-all">{value}</dd>
                          </Fragment>
                        ))}
                      </dl>
                    </CardContent>
                  </Card>
                )}

                <div className="relative min-h-[320px] flex-1 overflow-hidden rounded-lg border bg-muted">
                  {mesh && mesh.indices.length > 0 ? (
                    <Viewer positions={mesh.positions} indices={mesh.indices} theme={theme} />
                  ) : (
                    <p className="absolute inset-0 grid place-items-center text-sm text-muted-foreground">
                      No preview available for this format.
                    </p>
                  )}
                </div>
              </>
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-1 text-center">
                <h2 className="text-sm font-medium">Detail</h2>
                <p className="text-sm text-muted-foreground">Select a file to inspect its metadata and preview.</p>
              </div>
            )}

            {stats && (
              <Card size="sm">
                <CardHeader>
                  <CardTitle>Index</CardTitle>
                </CardHeader>
                <CardContent className="flex flex-col">
                  <StatRow label="Revisions" value={stats.revisions} />
                  <StatRow label="File revisions" value={stats.file_revisions} />
                  <StatRow label="Unique blobs" value={stats.unique_blobs} />
                  <StatRow label="Unique bytes" value={formatBytes(stats.unique_bytes)} />
                  <StatRow label="Referenced" value={formatBytes(stats.referenced_bytes)} />
                  <StatRow label="Dedup saved" value={formatBytes(stats.dedup_saved_bytes)} />
                </CardContent>
              </Card>
            )}
          </section>
        </main>
      </div>
    </TooltipProvider>
  );
}
