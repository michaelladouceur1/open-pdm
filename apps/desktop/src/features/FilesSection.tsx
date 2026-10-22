import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { formatBytes, shortOid } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useApp } from "./AppContext";
import ChangeRow from "./components/ChangeRow";
import ListRow from "./components/ListRow";
import SectionHeader from "./components/SectionHeader";
import ViewModeButton from "./components/ViewModeButton";
import { SECTION_MIN_HEIGHT } from "./constants";

// Files section of the left column. Shows the changed files for the open
// "Current" revision (with per-row staging) or the tracked files for a
// committed revision.
export default function FilesSection() {
  const {
    files,
    selectedFile,
    selectedRev,
    isCurrent,
    filesView,
    setFilesView,
    filesCollapsed,
    setFilesCollapsed,
    revisionsCollapsed,
    filesHeightPct,
    pickFile,
    toggleStaged,
  } = useApp();

  return (
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
      <SectionHeader
        title="Files"
        collapsed={filesCollapsed}
        onToggle={() => setFilesCollapsed((c) => !c)}
        actions={
          <ViewModeButton
            mode={filesView}
            onToggle={() => setFilesView((m) => (m === "detailed" ? "compact" : "detailed"))}
          />
        }
      >
        {selectedRev &&
          (isCurrent ? (
            <Badge variant="outline">current</Badge>
          ) : (
            <Badge variant="outline" className="font-mono">
              @ {shortOid(selectedRev)}
            </Badge>
          ))}
      </SectionHeader>
      {!filesCollapsed && (
        <ScrollArea className="min-h-0 flex-1">
          <div className="flex flex-col gap-1.5 px-3 pb-3">
            {files.length === 0 && (
              <p className="text-xs text-muted-foreground">{isCurrent ? "No changes." : "No files."}</p>
            )}
            {isCurrent
              ? files.map((file) => (
                  <ChangeRow
                    key={file.path}
                    file={file}
                    compact={filesView === "compact"}
                    active={selectedFile?.path === file.path}
                    onOpen={() => pickFile(file)}
                    onToggleStage={() => toggleStaged(file)}
                  />
                ))
              : files.map((file) =>
                  filesView === "compact" ? (
                    <ListRow
                      key={file.path}
                      compact
                      active={selectedFile?.path === file.path}
                      onClick={() => pickFile(file)}
                    >
                      <span className="min-w-0 flex-1 truncate text-[13px]">{file.path}</span>
                      <span className="shrink-0 text-[11px] text-muted-foreground">
                        {formatBytes(file.size)}
                      </span>
                    </ListRow>
                  ) : (
                    <ListRow
                      key={file.path}
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
  );
}
