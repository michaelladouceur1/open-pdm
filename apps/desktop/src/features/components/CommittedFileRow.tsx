import type { FileEntry } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";

// A file row for a committed revision. With `highlight` set (the "show all
// files" view) the files the revision changed/committed are highlighted yellow
// while the rest of the tree stays neutral; without it the row is neutral. A
// file carries a `status` exactly when the revision changed it (see
// `Project::annotated_files_at`).
export default function CommittedFileRow({
  file,
  active,
  compact,
  highlight,
  onOpen,
}: {
  file: FileEntry;
  active: boolean;
  compact: boolean;
  highlight: boolean;
  onOpen: () => void;
}) {
  const changed = highlight && file.status != null;
  return (
    <button
      type="button"
      onClick={onOpen}
      className={cn(
        "flex w-full rounded-lg border text-left transition-colors",
        compact ? "flex-row items-center gap-2 px-2.5 py-1" : "flex-col items-start gap-0.5 px-2.5 py-2",
        changed ? "border-yellow-500/40 bg-yellow-500/5" : "border-transparent bg-muted/40 hover:bg-muted",
        active && "ring-1 ring-primary/40",
      )}
    >
      <span
        className={cn(
          "text-[13px]",
          compact ? "min-w-0 flex-1 truncate" : "break-all",
          changed && "text-yellow-700 dark:text-yellow-400",
        )}
      >
        {file.path}
      </span>
      <span className="shrink-0 text-[11px] text-muted-foreground">
        {formatBytes(file.size)}
        {file.is_binary ? " · binary" : ""}
      </span>
    </button>
  );
}
