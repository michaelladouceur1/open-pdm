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
import { cn } from "@/lib/utils";

// A small confirmation modal. Controlled via `open`/`onOpenChange`: confirming
// runs `onConfirm` and closes, cancelling (or pressing Escape) just closes.
export default function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  destructive = false,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: ReactNode;
  confirmLabel: string;
  destructive?: boolean;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogPortal>
        <AlertDialogBackdrop />
        <AlertDialogPopup>
          <div className="flex flex-col gap-1">
            <AlertDialogTitle>{title}</AlertDialogTitle>
            <AlertDialogDescription>{description}</AlertDialogDescription>
          </div>
          <div className="flex justify-end gap-2">
            <AlertDialogClose className={cn(buttonVariants({ variant: "outline", size: "sm" }))}>
              Cancel
            </AlertDialogClose>
            <AlertDialogClose
              className={cn(
                buttonVariants({ variant: destructive ? "destructive" : "default", size: "sm" }),
              )}
              onClick={onConfirm}
            >
              {confirmLabel}
            </AlertDialogClose>
          </div>
        </AlertDialogPopup>
      </AlertDialogPortal>
    </AlertDialog>
  );
}
