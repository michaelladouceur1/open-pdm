import { EyeIcon, EyeOffIcon } from "lucide-react";
import { buttonVariants } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

// Icon button that toggles the Files section between showing only the selected
// revision's changed files and showing every project file.
export default function ShowAllFilesButton({
  active,
  onToggle,
}: {
  active: boolean;
  onToggle: () => void;
}) {
  const label = active ? "Show changed files only" : "Show all project files";
  return (
    <Tooltip>
      <TooltipTrigger
        className={cn(
          buttonVariants({ variant: "ghost", size: "icon-xs" }),
          active && "bg-accent text-accent-foreground",
        )}
        onClick={onToggle}
        aria-label={label}
        aria-pressed={active}
      >
        {active ? <EyeIcon /> : <EyeOffIcon />}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
