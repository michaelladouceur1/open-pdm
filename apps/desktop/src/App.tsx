import { TooltipProvider } from "@/components/ui/tooltip";
import { AppProvider } from "@/features/AppContext";
import DetailPanel from "@/features/DetailPanel";
import ErrorBanner from "@/features/ErrorBanner";
import Header from "@/features/Header";
import Sidebar from "@/features/Sidebar";

export default function App() {
  return (
    <TooltipProvider>
      <AppProvider>
        <div className="flex h-screen flex-col bg-background text-foreground">
          <Header />
          <ErrorBanner />
          <main className="grid min-h-0 flex-1 grid-cols-[minmax(240px,400px)_1fr]">
            <Sidebar />
            <DetailPanel />
          </main>
        </div>
      </AppProvider>
    </TooltipProvider>
  );
}
