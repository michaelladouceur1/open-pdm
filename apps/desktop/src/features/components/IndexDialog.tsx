import type { ReactNode } from "react";
import {
  AlertDialog,
  AlertDialogBackdrop,
  AlertDialogClose,
  AlertDialogDescription,
  AlertDialogPopup,
  AlertDialogPortal,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { buttonVariants } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useApp } from "../AppContext";

function StatRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between border-b py-1 text-[13px] last:border-0">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{value}</span>
    </div>
  );
}

// Modal shown when the header's Index button runs a project index. Reports
// progress while the scan runs, then the resulting database statistics.
export default function IndexDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const { stats, indexing, error } = useApp();

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogPortal>
        <AlertDialogBackdrop />
        <AlertDialogPopup>
          <div className="flex flex-col gap-1">
            <AlertDialogTitle>Index</AlertDialogTitle>
            <AlertDialogDescription>
              {indexing
                ? "Scanning the project's revision history and objects…"
                : "Project's revision history and objects."}
            </AlertDialogDescription>
          </div>

          {indexing ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner className="size-4" />
              Indexing…
            </div>
          ) : error ? (
            <p className="font-mono text-xs break-all text-destructive">{error}</p>
          ) : stats ? (
            <div className="flex flex-col">
              <StatRow label="Revisions" value={stats.revisions} />
              <StatRow label="File revisions" value={stats.file_revisions} />
              <StatRow label="Unique blobs" value={stats.unique_blobs} />
              <StatRow label="Unique bytes" value={formatBytes(stats.unique_bytes)} />
              <StatRow label="Referenced" value={formatBytes(stats.referenced_bytes)} />
              <StatRow label="Dedup saved" value={formatBytes(stats.dedup_saved_bytes)} />
            </div>
          ) : null}

          <div className="flex justify-end gap-2">
            <AlertDialogClose className={cn(buttonVariants({ variant: "outline", size: "sm" }))}>
              Close
            </AlertDialogClose>
          </div>
        </AlertDialogPopup>
      </AlertDialogPortal>
    </AlertDialog>
  );
}
