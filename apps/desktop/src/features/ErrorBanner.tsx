import { CircleAlertIcon } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { useApp } from "./AppContext";

// Error banner shown beneath the header. Renders nothing when there is no error.
export default function ErrorBanner() {
  const { error } = useApp();

  if (!error) return null;

  return (
    <div className="border-b px-3 py-2">
      <Alert variant="destructive">
        <CircleAlertIcon />
        <AlertTitle>Something went wrong</AlertTitle>
        <AlertDescription className="font-mono text-xs break-all">{error}</AlertDescription>
      </Alert>
    </div>
  );
}
