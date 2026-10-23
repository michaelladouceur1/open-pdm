import { RotateCcwIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Revision } from "@/lib/api";
import { cn } from "@/lib/utils";
import ConfirmDialog from "./ConfirmDialog";

// One committed version of the selected file in the detail panel's Revisions
// section. Clicking the row previews that version in the viewer; the trailing
// buttons check it out into the working tree or delete it from history, both
// behind a confirmation modal. The git hash sits on the right of the row.
export default function FileRevisionRow({
  revision,
  label,
  active,
  checkingOut,
  deleting,
  onPreview,
  onCheckout,
  onDelete,
}: {
  revision: Revision;
  label: string;
  active: boolean;
  checkingOut: boolean;
  deleting: boolean;
  onPreview: () => void;
  onCheckout: () => void;
  onDelete: () => void;
}) {
  const [confirming, setConfirming] = useState<"checkout" | "delete" | null>(null);
  const busy = checkingOut || deleting;
  const checkoutLabel = `Check out Rev ${label} into the working tree`;
  const deleteLabel = `Delete Rev ${label} from history`;

  return (
    <div
      className={cn(
        "group flex w-full items-center gap-2 rounded-lg border px-2 py-1 transition-colors",
        active ? "border-primary/40 bg-accent" : "border-transparent bg-muted/40 hover:bg-muted",
      )}
    >
      <button
        type="button"
        onClick={onPreview}
        title={revision.summary}
        className="flex min-w-0 flex-1 items-center gap-2 text-left"
      >
        <span className="shrink-0 text-[13px] font-medium">Rev {label}</span>
        <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
          {revision.summary}
        </span>
      </button>

      <Tooltip>
        <TooltipTrigger
          className={cn(
            buttonVariants({ variant: "ghost", size: "icon-xs" }),
            "opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100",
            active && "opacity-100",
          )}
          onClick={() => setConfirming("checkout")}
          disabled={busy}
          aria-label={checkoutLabel}
        >
          <RotateCcwIcon />
        </TooltipTrigger>
        <TooltipContent>{checkoutLabel}</TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger
          className={cn(
            buttonVariants({ variant: "ghost", size: "icon-xs" }),
            "text-destructive hover:text-destructive opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100",
            active && "opacity-100",
          )}
          onClick={() => setConfirming("delete")}
          disabled={busy}
          aria-label={deleteLabel}
        >
          <Trash2Icon />
        </TooltipTrigger>
        <TooltipContent>{deleteLabel}</TooltipContent>
      </Tooltip>

      <span className="shrink-0 font-mono text-[11px] text-muted-foreground">{revision.short}</span>

      <ConfirmDialog
        open={confirming === "checkout"}
        onOpenChange={(open) => {
          if (!open) setConfirming(null);
        }}
        title={`Check out Rev ${label}?`}
        description={`This replaces this file's current contents with ${revision.short} in the working tree. Only this file changes; the restored version is staged in the Current revision.`}
        confirmLabel="Check out"
        onConfirm={() => {
          setConfirming(null);
          onCheckout();
        }}
      />
      <ConfirmDialog
        open={confirming === "delete"}
        onOpenChange={(open) => {
          if (!open) setConfirming(null);
        }}
        title={`Delete Rev ${label}?`}
        description="This removes this version of the file from history and rewrites the commits after it. Commit hashes change and any uncommitted changes to this file are discarded. This cannot be undone."
        confirmLabel="Delete"
        destructive
        onConfirm={() => {
          setConfirming(null);
          onDelete();
        }}
      />
    </div>
  );
}
