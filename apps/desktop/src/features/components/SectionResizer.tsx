import { useApp } from "../AppContext";

// Draggable divider between the Files and Revisions sections. It resizes by
// pointer drag or arrow keys; the resize logic lives with the layout state in
// the app context.
export default function SectionResizer() {
  const { filesHeightPct, startResize, resize, endResize, resizeWithKeyboard } = useApp();
  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize Files and Revisions"
      aria-valuenow={Math.round(filesHeightPct)}
      tabIndex={0}
      onPointerDown={startResize}
      onPointerMove={resize}
      onPointerUp={endResize}
      onKeyDown={resizeWithKeyboard}
      className="group flex h-2 shrink-0 cursor-row-resize touch-none items-center justify-center border-y bg-muted/40 select-none focus-visible:ring-[3px] focus-visible:ring-ring/50 focus-visible:outline-none"
    >
      <div className="h-0.5 w-8 rounded-full bg-border transition-colors group-hover:bg-primary/50" />
    </div>
  );
}
