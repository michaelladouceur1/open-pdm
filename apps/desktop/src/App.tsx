import { Fragment, useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CircleAlertIcon,
  DatabaseIcon,
  FolderOpenIcon,
  FolderPlusIcon,
  MoonIcon,
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
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
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

type Theme = "light" | "dark";

function initialTheme(): Theme {
  const saved = localStorage.getItem("openpdm-theme");
  return saved === "dark" ? "dark" : "light";
}

// A selectable list row, used for both revisions and files.
function ListRow({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full flex-col items-start gap-0.5 rounded-lg border px-2.5 py-2 text-left transition-colors",
        active
          ? "border-primary/40 bg-accent text-accent-foreground"
          : "border-transparent bg-muted/40 hover:bg-muted",
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
  const [theme, setTheme] = useState<Theme>(initialTheme);

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
    localStorage.setItem("openpdm-theme", theme);
  }, [theme]);

  const themeLabel =
    theme === "light" ? "Switch to dark theme" : "Switch to light theme";

  return (
    <TooltipProvider>
      <div className="flex h-screen flex-col bg-background text-foreground">
        <header className="flex items-center gap-2 border-b bg-card px-3 py-2">
          <div className="px-1 text-sm font-semibold tracking-wide text-primary">
            OpenPDM
          </div>
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
              <AlertDescription className="font-mono text-xs break-all">
                {error}
              </AlertDescription>
            </Alert>
          </div>
        )}

        <main className="grid min-h-0 flex-1 grid-cols-[minmax(220px,260px)_minmax(240px,300px)_1fr]">
          <aside className="flex min-h-0 flex-col border-r">
            <div className="flex items-center justify-between px-3 pt-3 pb-2">
              <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
                Revisions
              </h2>
              {revisions.length > 0 && (
                <Badge variant="secondary">{revisions.length}</Badge>
              )}
            </div>
            <ScrollArea className="min-h-0 flex-1">
              <div className="flex flex-col gap-1.5 px-3 pb-3">
                {revisions.length === 0 && (
                  <p className="text-xs text-muted-foreground">
                    No revisions. Open a repository above.
                  </p>
                )}
                {revisions.map((revision) => (
                  <ListRow
                    key={revision.oid}
                    active={revision.oid === selectedRev}
                    onClick={() => pickRevision(revision.oid)}
                  >
                    <span className="font-mono text-xs text-muted-foreground">
                      {revision.short}
                    </span>
                    <span className="line-clamp-2 text-[13px]">
                      {revision.summary}
                    </span>
                    <span className="text-[11px] text-muted-foreground">
                      {revision.author} · {formatTime(revision.time)}
                    </span>
                  </ListRow>
                ))}
              </div>
            </ScrollArea>
          </aside>

          <section className="flex min-h-0 flex-col border-r">
            <div className="flex items-center gap-2 px-3 pt-3 pb-2">
              <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
                Files
              </h2>
              {selectedRev && (
                <Badge variant="outline" className="font-mono">
                  @ {shortOid(selectedRev)}
                </Badge>
              )}
            </div>
            <ScrollArea className="min-h-0 flex-1">
              <div className="flex flex-col gap-1.5 px-3 pb-3">
                {files.length === 0 && (
                  <p className="text-xs text-muted-foreground">No files.</p>
                )}
                {files.map((file) => (
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
                ))}
              </div>
            </ScrollArea>
          </section>

          <section className="flex min-h-0 flex-col gap-3 overflow-y-auto p-3">
            {selectedFile ? (
              <>
                <div className="flex flex-col gap-1.5">
                  <h1 className="font-mono text-sm break-all">
                    {selectedFile.path}
                  </h1>
                  <div className="flex items-center gap-1.5">
                    <Badge variant="secondary">
                      {formatBytes(selectedFile.size)}
                    </Badge>
                    {selectedFile.is_binary && (
                      <Badge variant="outline">binary</Badge>
                    )}
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
                        {Object.entries(metadata.attributes).map(
                          ([key, value]) => (
                            <Fragment key={key}>
                              <dt className="text-muted-foreground">{key}</dt>
                              <dd className="font-mono text-xs break-all">
                                {value}
                              </dd>
                            </Fragment>
                          ),
                        )}
                      </dl>
                    </CardContent>
                  </Card>
                )}

                <div className="relative min-h-[320px] flex-1 overflow-hidden rounded-lg border bg-muted">
                  {mesh && mesh.indices.length > 0 ? (
                    <Viewer
                      positions={mesh.positions}
                      indices={mesh.indices}
                      theme={theme}
                    />
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
                <p className="text-sm text-muted-foreground">
                  Select a file to inspect its metadata and preview.
                </p>
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
                  <StatRow
                    label="Unique bytes"
                    value={formatBytes(stats.unique_bytes)}
                  />
                  <StatRow
                    label="Referenced"
                    value={formatBytes(stats.referenced_bytes)}
                  />
                  <StatRow
                    label="Dedup saved"
                    value={formatBytes(stats.dedup_saved_bytes)}
                  />
                </CardContent>
              </Card>
            )}
          </section>
        </main>
      </div>
    </TooltipProvider>
  );
}
