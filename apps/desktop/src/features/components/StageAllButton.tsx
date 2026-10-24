import { PlusIcon } from "lucide-react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

// Icon button that stages every unstaged working-tree change into the open
// "Current" revision. Disabled when there is nothing left to stage.
export default function StageAllButton({
  disabled,
  onStage,
}: {
  disabled: boolean;
  onStage: () => void;
}) {
  const label = "Stage all changes";
  return (
    <Tooltip>
      <TooltipTrigger
        className={cn(buttonVariants({ variant: "ghost", size: "icon-xs" }))}
        onClick={onStage}
        disabled={disabled}
        aria-label={label}
      >
        <PlusIcon />
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
