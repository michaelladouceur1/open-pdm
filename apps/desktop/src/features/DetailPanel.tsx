import { Fragment, type ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { formatBytes } from "@/lib/format";
import Viewer from "@/lib/Viewer";
import { useApp } from "./AppContext";

function StatRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between border-b py-1 text-[13px] last:border-0">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium">{value}</span>
    </div>
  );
}

// Right-hand panel: the selected file's name, metadata and 3D preview, plus the
// project index statistics once an index has been built.
export default function DetailPanel() {
  const { selectedFile, metadata, mesh, theme, previewLoading, stats } = useApp();

  return (
    <section className="flex min-h-0 flex-col gap-3 overflow-y-auto p-3">
      {selectedFile ? (
        <>
          <div className="flex flex-col gap-1.5">
            <h1 className="font-mono text-sm break-all">{selectedFile.path}</h1>
            <div className="flex items-center gap-1.5">
              <Badge variant="secondary">{formatBytes(selectedFile.size)}</Badge>
              {selectedFile.is_binary && <Badge variant="outline">binary</Badge>}
            </div>
          </div>

          {metadata && (
            <Card size="sm">
              <CardHeader>
                <CardTitle>Metadata</CardTitle>
              </CardHeader>
              <CardContent className="max-h-[40vh] overflow-y-auto">
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
              </CardContent>
            </Card>
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
                  "No preview available for this format."
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
