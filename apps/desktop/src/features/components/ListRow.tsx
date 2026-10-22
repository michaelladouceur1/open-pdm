import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

// A selectable list row, used for both revisions and files. Rows stack their
// contents in "detailed" mode and collapse to a single line in "compact" mode.
export default function ListRow({
  active,
  onClick,
  compact = false,
  children,
}: {
  active: boolean;
  onClick: () => void;
  compact?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full rounded-lg border text-left transition-colors",
        compact ? "flex-row items-center gap-2 px-2.5 py-1" : "flex-col items-start gap-0.5 px-2.5 py-2",
        active ? "border-primary/40 bg-accent text-accent-foreground" : "border-transparent bg-muted/40 hover:bg-muted",
      )}
    >
      {children}
    </button>
  );
}
