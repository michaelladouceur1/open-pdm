import { CheckIcon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { buttonVariants } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { CURRENT_REV } from "@/lib/api";
import { formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useApp } from "./AppContext";
import ListRow from "./components/ListRow";
import SectionHeader from "./components/SectionHeader";
import ViewModeButton from "./components/ViewModeButton";

// Revisions section of the left column. Leads with the open "Current" revision
// (the uncommitted working tree) followed by committed revisions, newest first.
export default function RevisionsSection() {
  const {
    info,
    revisions,
    selectedRev,
    isCurrent,
    changeCount,
    revisionsView,
    setRevisionsView,
    revisionsCollapsed,
    setRevisionsCollapsed,
    pickRevision,
    commitMessage,
    setCommitMessage,
    stagedCount,
    committing,
    commitCurrent,
  } = useApp();

  const commitLabel =
    stagedCount > 0 ? "Commit staged changes to Current" : "No staged changes to commit";

  return (
    <section className={cn("flex min-h-0 flex-col", revisionsCollapsed ? "shrink-0" : "flex-1")}>
      <SectionHeader
        title="Revisions"
        collapsed={revisionsCollapsed}
        onToggle={() => setRevisionsCollapsed((c) => !c)}
        actions={
          <ViewModeButton
            mode={revisionsView}
            onToggle={() => setRevisionsView((m) => (m === "detailed" ? "compact" : "detailed"))}
          />
        }
      >
        {revisions.length > 0 && <Badge variant="secondary">{revisions.length}</Badge>}
      </SectionHeader>
      {!revisionsCollapsed && isCurrent && (
        <div className="flex shrink-0 items-stretch gap-1 px-3 pb-2">
          <Textarea
            rows={2}
            value={commitMessage}
            onChange={(e) => setCommitMessage(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void commitCurrent();
              }
            }}
            placeholder="Commit message"
            aria-label="Commit message"
            disabled={committing}
            className="min-h-0 flex-1 resize-none py-1.5 text-[13px]"
          />
          <Tooltip>
            <TooltipTrigger
              className={cn(buttonVariants({ variant: "ghost", size: "icon-xs" }), "self-end")}
              onClick={() => void commitCurrent()}
              disabled={committing}
              aria-label={commitLabel}
            >
              {committing ? <Spinner className="size-3.5" /> : <CheckIcon />}
            </TooltipTrigger>
            <TooltipContent>{commitLabel}</TooltipContent>
          </Tooltip>
        </div>
      )}
      {!revisionsCollapsed && (
        <ScrollArea className="min-h-0 flex-1">
          <div className="flex flex-col gap-1.5 px-3 pb-3">
            {info &&
              (revisionsView === "compact" ? (
                <ListRow compact active={isCurrent} onClick={() => pickRevision(CURRENT_REV)}>
                  <span className="min-w-[8ch] shrink-0 font-mono text-xs text-muted-foreground">open</span>
                  <span className="min-w-0 flex-1 truncate text-[13px]">Current</span>
                  <span className="shrink-0 text-[11px] text-muted-foreground">
                    {changeCount > 0 ? `${changeCount} changed` : "clean"}
                  </span>
                </ListRow>
              ) : (
                <ListRow active={isCurrent} onClick={() => pickRevision(CURRENT_REV)}>
                  <span className="font-mono text-xs text-muted-foreground">working tree</span>
                  <span className="text-[13px]">Current</span>
                  <span className="text-[11px] text-muted-foreground">
                    {changeCount > 0
                      ? `${changeCount} uncommitted change${changeCount === 1 ? "" : "s"}`
                      : "open revision · no changes"}
                  </span>
                </ListRow>
              ))}
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
  );
}
