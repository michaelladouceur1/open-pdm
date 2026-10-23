import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { formatBytes, revisionLabel } from "@/lib/format";
import { cn } from "@/lib/utils";
import Viewer from "@/lib/Viewer";
import { useApp } from "./AppContext";
import FileRevisionRow from "./components/FileRevisionRow";

// Detail-panel split geometry. The Revisions list is a fixed 400px and Metadata
// takes the rest; once the Metadata column would fall below 300px the two
// collapse into a single tabbed section instead of squashing side by side.
const REVISIONS_WIDTH = 500;
const SECTIONS_GAP = 12; // Tailwind `gap-3`
const METADATA_MIN_WIDTH = 300;
const STACK_THRESHOLD = REVISIONS_WIDTH + SECTIONS_GAP + METADATA_MIN_WIDTH;

type DetailSection = "revisions" | "metadata";

function StatRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between border-b py-1 text-[13px] last:border-0">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{value}</span>
    </div>
  );
}

// Right-hand panel: the selected file's name, its committed versions and
// metadata side by side, and a 3D preview, plus the project index statistics
// once an index has been built. When the panel is too narrow for both sections
// the Revisions and Metadata views share a single tabbed area instead.
export default function DetailPanel() {
  const {
    selectedFile,
    metadata,
    mesh,
    theme,
    previewLoading,
    stats,
    fileRevisions,
    previewRev,
    checkingOut,
    deletingRev,
    pickFileRev,
    checkoutFile,
    deleteFileRevision,
  } = useApp();

  const sectionRef = useRef<HTMLElement>(null);
  const [wide, setWide] = useState(true);
  const [activeSection, setActiveSection] = useState<DetailSection>("revisions");

  // Track the panel width so the two sections can collapse into a tabbed single
  // section once there is no longer room for a 400px list beside 300px of
  // metadata.
  useEffect(() => {
    const el = sectionRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWide(entry.contentRect.width >= STACK_THRESHOLD);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Formats the core can't parse itself (e.g. SolidWorks) advertise
  // `preview = unavailable` when no external converter is configured; explain
  // how to enable the preview instead of the generic message.
  const previewHint =
    metadata?.attributes.preview === "unavailable"
      ? `No preview available. Configure a converter in .openpdm/converters.json to preview ${metadata.format.toUpperCase()} files.`
      : "No preview available for this format.";

  const revisionsCard = (
    <Card size="sm" className="min-w-0">
      <CardHeader>
        <CardTitle>Revisions</CardTitle>
      </CardHeader>
      <CardContent className="max-h-[40vh] overflow-y-auto">
        {fileRevisions === null ? (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Spinner className="size-3.5" />
            Loading revisions…
          </div>
        ) : fileRevisions.length === 0 ? (
          <p className="text-xs text-muted-foreground">No committed revisions for this file yet.</p>
        ) : (
          <div className="flex flex-col gap-1">
            {fileRevisions.map((revision, index) => (
              <FileRevisionRow
                key={revision.oid}
                revision={revision}
                label={revisionLabel(fileRevisions.length - 1 - index)}
                active={revision.oid === previewRev}
                checkingOut={checkingOut}
                deleting={deletingRev}
                onPreview={() => void pickFileRev(revision.oid)}
                onCheckout={() => void checkoutFile(revision.oid)}
                onDelete={() => void deleteFileRevision(revision.oid)}
              />
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );

  const metadataCard = (
    <Card size="sm" className="min-w-0">
      <CardHeader>
        <CardTitle>Metadata</CardTitle>
      </CardHeader>
      <CardContent className="max-h-[40vh] overflow-y-auto">
        {metadata ? (
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13px]">
            <dt className="text-muted-foreground">Handler</dt>
            <dd>{metadata.format}</dd>
            {Object.entries(metadata.attributes).map(([key, value]) => (
              <Fragment key={key}>
                <dt className="text-muted-foreground">{key}</dt>
                <dd className="font-mono text-xs break-all">{value}</dd>
              </Fragment>
            ))}
          </dl>
        ) : (
          <p className="text-xs text-muted-foreground">
            {previewLoading ? "Loading metadata…" : "No metadata available."}
          </p>
        )}
      </CardContent>
    </Card>
  );

  return (
    <section ref={sectionRef} className="flex min-h-0 min-w-0 flex-col gap-3 overflow-y-auto p-3">
      {selectedFile ? (
        <>
          <div className="flex flex-col gap-1.5">
            <h1 className="font-mono text-sm break-all">{selectedFile.path}</h1>
            <div className="flex items-center gap-1.5">
              <Badge variant="secondary">{formatBytes(selectedFile.size)}</Badge>
              {selectedFile.is_binary && <Badge variant="outline">binary</Badge>}
            </div>
          </div>

          {wide ? (
            // Wide: a fixed 400px Revisions list beside the remaining Metadata.
            <div className="grid gap-3" style={{ gridTemplateColumns: `${REVISIONS_WIDTH}px minmax(0, 1fr)` }}>
              {revisionsCard}
              {metadataCard}
            </div>
          ) : (
            // Narrow: one section at a time, chosen with the tabs above it.
            <div className="flex flex-col gap-2">
              <div className="flex items-center gap-1">
                {(["revisions", "metadata"] as const).map((section) => (
                  <button
                    key={section}
                    type="button"
                    onClick={() => setActiveSection(section)}
                    aria-pressed={activeSection === section}
                    className={cn(
                      buttonVariants({
                        variant: activeSection === section ? "secondary" : "ghost",
                        size: "xs",
                      }),
                      "capitalize",
                    )}
                  >
                    {section}
                  </button>
                ))}
              </div>
              {activeSection === "revisions" ? revisionsCard : metadataCard}
            </div>
          )}

          <div className="relative min-h-[320px] flex-1 overflow-hidden rounded-lg border bg-muted">
            {mesh && mesh.indices.length > 0 ? (
              <Viewer positions={mesh.positions} indices={mesh.indices} theme={theme} />
            ) : (
              <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 text-sm text-muted-foreground">
                {previewLoading ? (
                  <>
                    <Spinner className="size-4" />
                    Building preview…
                  </>
                ) : (
                  <span className="max-w-[85%] text-center">{previewHint}</span>
                )}
              </div>
            )}
          </div>
        </>
      ) : (
        <div className="flex h-full flex-col items-center justify-center gap-1 text-center">
          <h2 className="text-sm font-medium">Detail</h2>
          <p className="text-sm text-muted-foreground">Select a file to inspect its metadata and preview.</p>
        </div>
      )}

      {stats && (
        <Card size="sm">
          <CardHeader>
            <CardTitle>Index</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col">
            <StatRow label="Revisions" value={stats.revisions} />
            <StatRow label="File revisions" value={stats.file_revisions} />
            <StatRow label="Unique blobs" value={stats.unique_blobs} />
            <StatRow label="Unique bytes" value={formatBytes(stats.unique_bytes)} />
            <StatRow label="Referenced" value={formatBytes(stats.referenced_bytes)} />
            <StatRow label="Dedup saved" value={formatBytes(stats.dedup_saved_bytes)} />
          </CardContent>
        </Card>
      )}
    </section>
  );
}
