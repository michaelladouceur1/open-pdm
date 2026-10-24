import { MinusIcon } from "lucide-react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

// Icon button that unstages every staged change from the open "Current"
// revision. Disabled when there is nothing staged.
export default function UnstageAllButton({
  disabled,
  onUnstage,
}: {
  disabled: boolean;
  onUnstage: () => void;
}) {
  const label = "Unstage all changes";
  return (
    <Tooltip>
      <TooltipTrigger
        className={cn(buttonVariants({ variant: "ghost", size: "icon-xs" }))}
        onClick={onUnstage}
        disabled={disabled}
        aria-label={label}
      >
        <MinusIcon />
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
