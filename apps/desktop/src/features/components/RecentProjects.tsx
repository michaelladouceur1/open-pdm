import { FolderIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { MAX_RECENT_PROJECTS } from "../constants";
import { useApp } from "../AppContext";

// Dropdown contents for the header's project path box: the most recently
// opened projects, newest first, each loading its project on click. The open
// project is highlighted so it is clear which entry is already loaded.
export default function RecentProjects({ onPick }: { onPick: (path: string) => void }) {
  const { recentProjects, projectPath } = useApp();
  const paths = recentProjects.slice(0, MAX_RECENT_PROJECTS);

  if (paths.length === 0) {
    return <p className="px-2 py-1.5 text-xs text-muted-foreground">No recent projects yet.</p>;
  }

  return (
    <div className="flex flex-col gap-0.5">
      <div className="px-2 pt-0.5 pb-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        Recent projects
      </div>
      {paths.map((path) => {
        const active = path === projectPath;
        return (
          <button
            key={path}
            type="button"
            onClick={() => onPick(path)}
            title={path}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
              active ? "bg-accent text-accent-foreground" : "hover:bg-muted",
            )}
          >
            <FolderIcon className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate font-mono text-xs">{path}</span>
          </button>
        );
      })}
    </div>
  );
}
