import { CheckIcon, RotateCcwIcon } from "lucide-react";
import { useState } from "react";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import ConfirmDialog from "./ConfirmDialog";
import ListRow from "./ListRow";

// The leading "Current" entry in the Revisions list (the open working tree).
// Selecting it browses the worktree; the trailing checkout button restores the
// tracked tree to HEAD and marks Current as the checked-out revision.
export default function CurrentRevisionRow({
  compact,
  active,
  checkedOut,
  changeCount,
  checkingOut,
  disabled,
  onSelect,
  onCheckout,
}: {
  compact: boolean;
  active: boolean;
  checkedOut: boolean;
  changeCount: number;
  checkingOut: boolean;
  disabled: boolean;
  onSelect: () => void;
  onCheckout: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const checkoutLabel = "Check out the Current revision";
  const checkIcon = checkedOut && <CheckIcon className="size-3 text-primary" aria-label="Checked out" />;

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
            <span className="flex min-w-[8ch] shrink-0 items-center gap-1 font-mono text-xs text-muted-foreground">
              {checkIcon}
              open
            </span>
            <span className="min-w-0 flex-1 truncate text-[13px]">Current</span>
            <span className="shrink-0 text-[11px] text-muted-foreground">
              {changeCount > 0 ? `${changeCount} changed` : "clean"}
            </span>
          </>
        ) : (
          <>
            <span className="flex items-center gap-1 font-mono text-xs text-muted-foreground">
              {checkIcon}
              working tree
            </span>
            <span className="text-[13px]">Current</span>
            <span className="text-[11px] text-muted-foreground">
              {changeCount > 0
                ? `${changeCount} uncommitted change${changeCount === 1 ? "" : "s"}`
                : "open revision · no changes"}
            </span>
          </>
        )}
      </ListRow>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Check out the Current revision?"
        description="This replaces the working tree with HEAD, discarding all staged and unstaged changes to tracked files. Untracked files are left alone, and the Current revision is marked as checked out."
        confirmLabel="Check out"
        onConfirm={() => {
          setConfirming(false);
          onCheckout();
        }}
      />
    </>
  );
}
