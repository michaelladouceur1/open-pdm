import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { shortOid } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useApp } from "./AppContext";
import ChangeRow from "./components/ChangeRow";
import CommittedFileRow from "./components/CommittedFileRow";
import SectionHeader from "./components/SectionHeader";
import ShowAllFilesButton from "./components/ShowAllFilesButton";
import StageAllButton from "./components/StageAllButton";
import UnstageAllButton from "./components/UnstageAllButton";
import ViewModeButton from "./components/ViewModeButton";
import { SECTION_MIN_HEIGHT } from "./constants";

// Files section of the left column. Shows the changed files for the open
// revision (with per-row staging for the "Current" revision). The "show all"
// toggle widens this to every project file: the whole working tree for the
// Current revision, or the whole committed tree for an earlier revision.
export default function FilesSection() {
  const {
    files,
    selectedFile,
    selectedRev,
    isCurrent,
    filesView,
    setFilesView,
    showAllFiles,
    toggleShowAllFiles,
    filesCollapsed,
    setFilesCollapsed,
    revisionsCollapsed,
    filesHeightPct,
    pickFile,
    toggleStaged,
    stageAll,
    unstageAll,
  } = useApp();

  // Staging only applies to the open "Current" revision. A change counts as
  // stageable when it differs from the working tree and is not yet staged.
  const canStageAll = isCurrent && files.some((file) => file.status != null && !file.staged);
  const canUnstageAll = isCurrent && files.some((file) => file.staged === true);

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
          <>
            <StageAllButton disabled={!canStageAll} onStage={() => void stageAll()} />
            <UnstageAllButton disabled={!canUnstageAll} onUnstage={() => void unstageAll()} />
            <ShowAllFilesButton active={showAllFiles} onToggle={() => void toggleShowAllFiles()} />
            <ViewModeButton
              mode={filesView}
              onToggle={() => setFilesView((m) => (m === "detailed" ? "compact" : "detailed"))}
            />
          </>
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
          <div className="flex flex-col gap-1.5 px-3 pt-1 pb-3">
            {files.length === 0 && (
              <p className="text-xs text-muted-foreground">
                {showAllFiles ? "No files." : "No changes."}
              </p>
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
              : files.map((file) => (
                  <CommittedFileRow
                    key={file.path}
                    file={file}
                    compact={filesView === "compact"}
                    highlight={showAllFiles}
                    active={selectedFile?.path === file.path}
                    onOpen={() => pickFile(file)}
                  />
                ))}
          </div>
        </ScrollArea>
      )}
    </section>
  );
}
