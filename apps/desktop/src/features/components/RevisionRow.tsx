import { CheckIcon, RotateCcwIcon } from "lucide-react";
import { useState } from "react";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Revision } from "@/lib/api";
import { formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import ConfirmDialog from "./ConfirmDialog";
import ListRow from "./ListRow";

// One committed revision in the left column's Revisions list. Selecting the row
// browses that commit; the trailing checkout button restores the whole commit
// tree into the working tree (without moving HEAD). A checked-out commit keeps
// a distinct highlight even while a different revision is selected.
export default function RevisionRow({
  revision,
  compact,
  active,
  checkedOut,
  checkingOut,
  disabled,
  onSelect,
  onCheckout,
}: {
  revision: Revision;
  compact: boolean;
  active: boolean;
  checkedOut: boolean;
  checkingOut: boolean;
  disabled: boolean;
  onSelect: () => void;
  onCheckout: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const checkoutLabel = `Check out commit ${revision.short} into the working tree`;

  return (
    <>
      <ListRow
        compact={compact}
        active={active}
        checkedOut={checkedOut}
        onClick={onSelect}
        actions={
          <Tooltip>
            <TooltipTrigger
              className={cn(
                buttonVariants({ variant: "ghost", size: "icon-xs" }),
                "opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100",
                checkedOut && "opacity-100",
              )}
              onClick={() => setConfirming(true)}
              disabled={disabled}
              aria-label={checkoutLabel}
            >
              {checkingOut ? <Spinner className="size-3.5" /> : <RotateCcwIcon />}
            </TooltipTrigger>
            <TooltipContent>{checkoutLabel}</TooltipContent>
          </Tooltip>
        }
      >
        {compact ? (
          <>
            <span className="flex shrink-0 items-center gap-1 font-mono text-xs text-muted-foreground">
              {checkedOut && <CheckIcon className="size-3 text-primary" aria-label="Checked out" />}
              {revision.short}
            </span>
            <span className="min-w-0 flex-1 truncate text-[13px]">{revision.summary}</span>
            <span className="shrink-0 text-[11px] text-muted-foreground">{formatTime(revision.time)}</span>
          </>
        ) : (
          <>
            <span className="flex items-center gap-1 font-mono text-xs text-muted-foreground">
              {checkedOut && <CheckIcon className="size-3 text-primary" aria-label="Checked out" />}
              {revision.short}
            </span>
            <span className="line-clamp-2 text-[13px]">{revision.summary}</span>
            <span className="text-[11px] text-muted-foreground">
              {revision.author} · {formatTime(revision.time)}
            </span>
          </>
        )}
      </ListRow>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title={`Check out commit ${revision.short}?`}
        description={`This replaces the working tree with the contents of ${revision.short} ("${revision.summary}"). Uncommitted changes to tracked files are discarded, the restored files are staged in the Current revision, and HEAD is not moved. Untracked files are left alone.`}
        confirmLabel="Check out"
        onConfirm={() => {
          setConfirming(false);
          onCheckout();
        }}
      />
    </>
  );
}
