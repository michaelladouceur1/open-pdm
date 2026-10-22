import { Rows2Icon, Rows4Icon } from "lucide-react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { ViewMode } from "../AppContext";

// Icon button that flips a section between detailed and compact rows.
export default function ViewModeButton({ mode, onToggle }: { mode: ViewMode; onToggle: () => void }) {
  const detailed = mode === "detailed";
  const label = detailed ? "Switch to compact view" : "Switch to detailed view";
  return (
    <Tooltip>
      <TooltipTrigger
        className={cn(buttonVariants({ variant: "ghost", size: "icon-xs" }))}
        onClick={onToggle}
        aria-label={label}
      >
        {detailed ? <Rows2Icon /> : <Rows4Icon />}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
