import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

// A selectable list row, used for both revisions and files. Rows stack their
// contents in "detailed" mode and collapse to a single line in "compact" mode.
// Optional trailing actions (e.g. the checkout button) sit outside the row's
// selectable area, so they never nest buttons inside buttons.
export default function ListRow({
  active,
  checkedOut = false,
  onClick,
  compact = false,
  actions,
  children,
}: {
  active: boolean;
  checkedOut?: boolean;
  onClick: () => void;
  compact?: boolean;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div
      className={cn(
        "group flex w-full items-stretch gap-1 rounded-lg border transition-colors",
        checkedOut
          ? active
            ? "border-primary bg-accent text-accent-foreground"
            : "border-primary/60 bg-primary/5"
          : active
            ? "border-primary/40 bg-accent text-accent-foreground"
            : "border-transparent bg-muted/40 hover:bg-muted",
      )}
    >
      <button
        type="button"
        onClick={onClick}
        aria-current={checkedOut ? "true" : undefined}
        className={cn(
          "flex min-w-0 flex-1 text-left",
          compact ? "flex-row items-center gap-2 px-2.5 py-1" : "flex-col items-start gap-0.5 px-2.5 py-2",
        )}
      >
        {children}
      </button>
      {actions && <div className="flex shrink-0 items-center gap-0.5 pr-1">{actions}</div>}
    </div>
  );
}
