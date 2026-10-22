import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
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
  } = useApp();

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
