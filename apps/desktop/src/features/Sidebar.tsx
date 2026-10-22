import { useApp } from "./AppContext";
import FilesSection from "./FilesSection";
import RevisionsSection from "./RevisionsSection";
import SectionResizer from "./components/SectionResizer";

// Left column: the Files and Revisions sections stacked, with a draggable
// divider between them. The divider only appears when both are expanded.
export default function Sidebar() {
  const { leftColumnRef, filesCollapsed, revisionsCollapsed } = useApp();

  return (
    <aside ref={leftColumnRef} className="flex min-h-0 flex-col border-r">
      <FilesSection />
      {!filesCollapsed && !revisionsCollapsed && <SectionResizer />}
      <RevisionsSection />
    </aside>
  );
}
