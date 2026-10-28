import { useRef, useState } from "react";
import {
  ChevronDownIcon,
  FolderOpenIcon,
  FolderPlusIcon,
  MoonIcon,
  SettingsIcon,
  SunIcon,
} from "lucide-react";
import { Button, buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Separator } from "@/components/ui/separator";
import { Spinner } from "@/components/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { useApp } from "./AppContext";
import RecentProjects from "./components/RecentProjects";
import SettingsDialog from "./components/SettingsDialog";

// Top bar: product mark, editable project path (with a recent-projects
// dropdown), project actions, the theme toggle, settings and a busy indicator
// while a project operation is in flight.
export default function Header() {
  const {
    projectPath,
    setProjectPath,
    loadPath,
    newProject,
    openProjectPicker,
    theme,
    setTheme,
    busy,
    openRecent,
  } = useApp();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [recentOpen, setRecentOpen] = useState(false);
  // The popover is anchored to the whole path field so the menu lines up with
  // the input rather than just the narrow chevron button.
  const pathFieldRef = useRef<HTMLDivElement>(null);

  const themeLabel = theme === "light" ? "Switch to dark theme" : "Switch to light theme";

  return (
    <>
      <header className="flex items-center gap-2 border-b bg-card px-3 py-2">
        <div className="px-1 text-sm font-semibold tracking-wide text-primary">OpenPDM</div>
        <Separator orientation="vertical" className="mx-0.5 h-5" />
        <Popover open={recentOpen} onOpenChange={setRecentOpen}>
          <div ref={pathFieldRef} className="relative min-w-0 flex-1">
            <Input
              className="h-8 w-full pr-8 font-mono"
              value={projectPath}
              placeholder="/path/to/project"
              onChange={(e) => setProjectPath(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter") return;
                setRecentOpen(false);
                void loadPath(projectPath);
              }}
            />
            <PopoverTrigger
              aria-label="Show recent projects"
              title="Recent projects"
              className="absolute inset-y-0 right-0 flex w-8 items-center justify-center rounded-r-lg text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
            >
              <ChevronDownIcon className="size-4" />
            </PopoverTrigger>
          </div>
          <PopoverContent
            anchor={pathFieldRef}
            align="start"
            sideOffset={4}
            className="w-(--anchor-width) p-1"
          >
            <RecentProjects
              onPick={(path) => {
                setRecentOpen(false);
                void openRecent(path);
              }}
            />
          </PopoverContent>
        </Popover>
        <Button onClick={newProject} disabled={busy}>
          <FolderPlusIcon data-icon="inline-start" />
          New
        </Button>
        <Button variant="outline" onClick={openProjectPicker} disabled={busy}>
          <FolderOpenIcon data-icon="inline-start" />
          Open
        </Button>
        <Tooltip>
          <TooltipTrigger
            className={cn(buttonVariants({ variant: "outline", size: "icon" }))}
            onClick={() => setSettingsOpen(true)}
            aria-label="Settings"
          >
            <SettingsIcon />
          </TooltipTrigger>
          <TooltipContent>Settings</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger
            className={cn(buttonVariants({ variant: "outline", size: "icon" }))}
            onClick={() => setTheme((t) => (t === "light" ? "dark" : "light"))}
            aria-label={themeLabel}
          >
            {theme === "light" ? <SunIcon /> : <MoonIcon />}
          </TooltipTrigger>
          <TooltipContent>{themeLabel}</TooltipContent>
        </Tooltip>
        {busy && (
          <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Spinner className="size-3.5" />
            working…
          </span>
        )}
      </header>
      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
    </>
  );
}
