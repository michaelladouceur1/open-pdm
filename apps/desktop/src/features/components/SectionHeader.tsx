import { ChevronRightIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

// Collapsible header for a stacked section. The title row toggles the section;
// optional actions render right-aligned on the same row.
export default function SectionHeader({
  title,
  collapsed,
  onToggle,
  actions,
  children,
}: {
  title: string;
  collapsed: boolean;
  onToggle: () => void;
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="flex w-full shrink-0 items-center gap-2 px-3 pt-3 pb-2">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={!collapsed}
        className="flex min-w-0 flex-1 items-center gap-2 text-left"
      >
        <ChevronRightIcon
          className={cn("size-3.5 shrink-0 text-muted-foreground transition-transform", !collapsed && "rotate-90")}
        />
        <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">{title}</h2>
        {children}
      </button>
      {actions && <SectionActions>{actions}</SectionActions>}
    </div>
  );
}

// Container for per-section actions, rendered at the right of the section header.
function SectionActions({ children }: { children: ReactNode }) {
  return <div className="flex shrink-0 items-center gap-1 text-muted-foreground">{children}</div>;
}
