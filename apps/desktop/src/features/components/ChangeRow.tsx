import { MinusIcon, PlusIcon, Undo2Icon } from "lucide-react";
import { useState } from "react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { FileEntry } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import ConfirmDialog from "./ConfirmDialog";

// A file row for the open "Current" revision. Changes already staged into the
// revision are highlighted green; changes not yet included are highlighted red.
// Trailing icon buttons stage/unstage the file and revert it to its committed
// state without opening it.
//
// In "show all files" mode unchanged (committed) files are listed too; they are
// left neutral and get no staging button.
export default function ChangeRow({
  file,
  active,
  compact,
  onOpen,
  onToggleStage,
  onRevert,
}: {
  file: FileEntry;
  active: boolean;
  compact: boolean;
  onOpen: () => void;
  onToggleStage: () => void;
  onRevert: () => void;
}) {
  const [confirmingRevert, setConfirmingRevert] = useState(false);
  const changed = file.status != null || file.staged != null;
  const staged = file.staged === true;
  const label = staged ? "Unstage from current revision" : "Add to current revision";
  const revertLabel = "Revert to committed version";
  return (
    <div
      className={cn(
        "flex w-full items-stretch rounded-lg border transition-colors",
        !changed && "border-transparent bg-muted/40",
        changed && (staged ? "border-emerald-500/40 bg-emerald-500/5" : "border-destructive/40 bg-destructive/5"),
        active && "ring-1 ring-primary/40",
      )}
    >
      <button
        type="button"
        onClick={onOpen}
        className={cn(
          "flex min-w-0 flex-1 rounded-lg text-left",
          compact ? "flex-row items-center gap-2 px-2.5 py-1" : "flex-col items-start gap-0.5 px-2.5 py-2",
        )}
      >
        <span
          className={cn(
            "text-[13px]",
            compact ? "min-w-0 flex-1 truncate" : "break-all",
            changed && (staged ? "text-emerald-700 dark:text-emerald-400" : "text-destructive"),
          )}
        >
          {file.path}
        </span>
        <span className="shrink-0 text-[11px] text-muted-foreground">
          {changed ? `${file.status ?? "changed"} · ${formatBytes(file.size)}` : formatBytes(file.size)}
          {file.is_binary ? " · binary" : ""}
        </span>
      </button>
      {changed && (
        <>
          <Tooltip>
            <TooltipTrigger
              className={cn(buttonVariants({ variant: "ghost", size: "icon-xs" }), "mr-1 self-center")}
              onClick={onToggleStage}
              aria-label={label}
            >
              {staged ? <MinusIcon /> : <PlusIcon />}
            </TooltipTrigger>
            <TooltipContent>{label}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger
              className={cn(
                buttonVariants({ variant: "ghost", size: "icon-xs" }),
                "mr-1 self-center text-destructive hover:text-destructive",
              )}
              onClick={() => setConfirmingRevert(true)}
              aria-label={revertLabel}
            >
              <Undo2Icon />
            </TooltipTrigger>
            <TooltipContent>{revertLabel}</TooltipContent>
          </Tooltip>
          <ConfirmDialog
            open={confirmingRevert}
            onOpenChange={setConfirmingRevert}
            title="Revert this file?"
            description={`This discards the uncommitted changes to ${file.path}, restoring it to its committed version. This cannot be undone.`}
            confirmLabel="Revert"
            destructive
            onConfirm={onRevert}
          />
        </>
      )}
    </div>
  );
}
