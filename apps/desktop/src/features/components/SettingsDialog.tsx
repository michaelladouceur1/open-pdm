import { useEffect, type ReactNode } from "react";
import {
  Dialog,
  DialogBackdrop,
  DialogClose,
  DialogPopup,
  DialogPortal,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button, buttonVariants } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Spinner } from "@/components/ui/spinner";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/components/ui/tabs";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useApp } from "../AppContext";

function StatRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between border-b py-1 text-[13px] last:border-0">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{value}</span>
    </div>
  );
}

/** Format a byte count as megabytes, e.g. `12.3 MB`. */
function formatMegabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// A grid of checkboxes, one per trackable extension. Checked means "tracked";
// `disabled` marks types excluded by a broader scope that this one cannot
// override.
function ExtensionPicker({
  extensions,
  ignored,
  disabled,
  onToggle,
}: {
  extensions: string[];
  ignored: Set<string>;
  disabled?: Set<string>;
  onToggle: (extension: string) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3">
      {extensions.map((extension) => {
        const blocked = disabled?.has(extension) ?? false;
        return (
          <label
            key={extension}
            className={cn(
              "flex items-center gap-2 text-sm select-none",
              blocked && "cursor-not-allowed opacity-50",
            )}
          >
            <Checkbox
              checked={!blocked && !ignored.has(extension)}
              disabled={blocked}
              onCheckedChange={() => onToggle(extension)}
            />
            <span className="font-mono">.{extension}</span>
          </label>
        );
      })}
    </div>
  );
}

// The settings dialog opened from the header's settings button. Vertical tabs
// on the left switch between app-level settings, project-level settings and the
// current project's index.
export default function SettingsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const {
    appSettings,
    projectSettings,
    trackableExtensions,
    updateAppSettings,
    updateProjectSettings,
    info,
    stats,
    indexing,
    buildIndex,
    loadIndexStats,
    cacheSize,
    clearingCache,
    loadCacheSize,
    clearCache,
  } = useApp();

  // Show the index's current state and cache size each time the dialog opens;
  // building the index and clearing the cache are explicit actions.
  useEffect(() => {
    if (!open) return;
    void loadIndexStats();
    void loadCacheSize();
  }, [open]);

  const appIgnored = new Set(appSettings.ignored_extensions);
  const projectIgnored = new Set(projectSettings.ignored_extensions);

  function toggleApp(extension: string) {
    const next = new Set(appIgnored);
    if (next.has(extension)) next.delete(extension);
    else next.add(extension);
    void updateAppSettings({ ignored_extensions: [...next].sort() });
  }

  function toggleProject(extension: string) {
    const next = new Set(projectIgnored);
    if (next.has(extension)) next.delete(extension);
    else next.add(extension);
    void updateProjectSettings({ ignored_extensions: [...next].sort() });
  }

  const appTracked = trackableExtensions.length - appIgnored.size;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogPortal>
        <DialogBackdrop />
        <DialogPopup className="max-w-3xl">
          <div className="flex flex-col gap-1">
            <DialogTitle>Settings</DialogTitle>
          </div>

          <Tabs defaultValue="app" orientation="vertical" className="h-[26rem] gap-5">
            <TabsList className="min-w-28 self-start">
              <TabsTab value="app">App</TabsTab>
              <TabsTab value="project">Project</TabsTab>
              <TabsTab value="index">Index</TabsTab>
            </TabsList>

            <TabsPanel value="app" className="min-w-0 flex-1 overflow-y-auto pr-1">
              <div className="flex flex-col gap-3">
                <div>
                  <h3 className="text-sm font-medium">Tracked file types (App)</h3>
                </div>
                <ExtensionPicker
                  extensions={trackableExtensions}
                  ignored={appIgnored}
                  onToggle={toggleApp}
                />

                <div className="flex items-center justify-between gap-3 border-t pt-3">
                  <div>
                    <div className="text-sm font-medium">App cache</div>
                    <div className="text-sm text-muted-foreground">
                      {cacheSize === null
                        ? "Calculating…"
                        : `${formatMegabytes(cacheSize)} used by converted previews.`}
                    </div>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void clearCache()}
                    disabled={clearingCache || cacheSize === null || cacheSize === 0}
                  >
                    {clearingCache ? "Clearing…" : "Clear cache"}
                  </Button>
                </div>
              </div>
            </TabsPanel>

            <TabsPanel value="project" className="min-w-0 flex-1 overflow-y-auto pr-1">
              {info ? (
                <div className="flex flex-col gap-3">
                  <div>
                    <h3 className="text-sm font-medium">Tracked file types (Project)</h3>
                  </div>
                  <ExtensionPicker
                    extensions={trackableExtensions}
                    ignored={projectIgnored}
                    disabled={appIgnored}
                    onToggle={toggleProject}
                  />
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  Open a project to configure its tracked file types.
                </p>
              )}
            </TabsPanel>

            <TabsPanel value="index" className="min-w-0 flex-1 overflow-y-auto pr-1">
              <div className="flex flex-col gap-3">
                <div>
                  <h3 className="text-sm font-medium">Project index</h3>
                  <p className="text-sm text-muted-foreground">
                    The content-addressed cache of the project's revision history.
                  </p>
                </div>

                {indexing ? (
                  <div className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Spinner className="size-4" />
                    Indexing…
                  </div>
                ) : stats ? (
                  <div className="flex flex-col">
                    <StatRow label="Revisions" value={stats.revisions} />
                    <StatRow label="File revisions" value={stats.file_revisions} />
                    <StatRow label="Unique blobs" value={stats.unique_blobs} />
                    <StatRow label="Unique bytes" value={formatBytes(stats.unique_bytes)} />
                    <StatRow label="Referenced" value={formatBytes(stats.referenced_bytes)} />
                    <StatRow label="Dedup saved" value={formatBytes(stats.dedup_saved_bytes)} />
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">
                    {info
                      ? "This project has not been indexed yet."
                      : "Open a project to build its index."}
                  </p>
                )}

                <div>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void buildIndex()}
                    disabled={indexing || !info}
                  >
                    {stats ? "Rebuild index" : "Build index"}
                  </Button>
                </div>
              </div>
            </TabsPanel>
          </Tabs>

          <div className="flex justify-end">
            <DialogClose className={cn(buttonVariants({ variant: "outline", size: "sm" }))}>
              Close
            </DialogClose>
          </div>
        </DialogPopup>
      </DialogPortal>
    </Dialog>
  );
}
