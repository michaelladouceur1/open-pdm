import { CircleCheckIcon } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { useApp } from "./AppContext";

// Transient success banner shown beneath the header (e.g. after a push sync).
// Errors use the separate ErrorBanner; both are cleared as new operations run.
export default function NoticeBanner() {
  const { notice } = useApp();

  if (!notice) return null;

  return (
    <div className="border-b px-3 py-2">
      <Alert className="border-emerald-600/30 bg-emerald-500/10 text-emerald-800 dark:text-emerald-300">
        <CircleCheckIcon />
        <AlertTitle>Sync complete</AlertTitle>
        <AlertDescription className="break-all text-emerald-700 dark:text-emerald-400">
          {notice}
        </AlertDescription>
      </Alert>
    </div>
  );
}
