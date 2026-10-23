import { RotateCcwIcon } from "lucide-react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import type { Revision } from "@/lib/api";
import { cn } from "@/lib/utils";

// One committed version of the selected file in the detail panel's Revisions
// section. Clicking the row previews that version in the viewer; the trailing
// button checks it out into the working tree (restoring only this file). The
// git hash sits on the right of the row.
export default function FileRevisionRow({
  revision,
  label,
  active,
  checkingOut,
  onPreview,
  onCheckout,
}: {
  revision: Revision;
  label: string;
  active: boolean;
  checkingOut: boolean;
  onPreview: () => void;
  onCheckout: () => void;
}) {
  const checkoutLabel = `Check out Rev ${label} into the working tree`;
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
          onClick={onCheckout}
          disabled={checkingOut}
          aria-label={checkoutLabel}
        >
          <RotateCcwIcon />
        </TooltipTrigger>
        <TooltipContent>{checkoutLabel}</TooltipContent>
      </Tooltip>
      <span className="shrink-0 font-mono text-[11px] text-muted-foreground">{revision.short}</span>
    </div>
  );
}
