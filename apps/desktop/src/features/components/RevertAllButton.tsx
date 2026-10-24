import { Undo2Icon } from "lucide-react";
import { useState } from "react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import ConfirmDialog from "./ConfirmDialog";

// Icon button that discards every working-tree change, restoring the files to
// their committed state. Disabled when there are no changes; the destructive
// action is gated behind a confirmation modal.
export default function RevertAllButton({
  disabled,
  onRevert,
}: {
  disabled: boolean;
  onRevert: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const label = "Revert all changes";
  return (
    <>
      <Tooltip>
        <TooltipTrigger
          className={cn(
            buttonVariants({ variant: "ghost", size: "icon-xs" }),
            "text-destructive hover:text-destructive",
          )}
          onClick={() => setConfirming(true)}
          disabled={disabled}
          aria-label={label}
        >
          <Undo2Icon />
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Revert all changes?"
        description="This discards every uncommitted change in the working tree, restoring the files to their committed state. This cannot be undone."
        confirmLabel="Revert all"
        destructive
        onConfirm={onRevert}
      />
    </>
  );
}
