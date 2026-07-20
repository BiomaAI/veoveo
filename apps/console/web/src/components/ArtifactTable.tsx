import { Eye } from "lucide-react";
import { artifactPreviewLabel } from "../artifactPreview";
import { EmptyState, StatusPill } from "./primitives";
import { formatBytes, formatDate } from "../format";
import type { ArtifactSummary } from "../types";

export function ArtifactTable({
  artifacts,
  onSelect,
  compact = false
}: {
  artifacts: ArtifactSummary[];
  onSelect: (artifact: ArtifactSummary) => void;
  compact?: boolean;
}) {
  if (!artifacts.length) return <EmptyState>No artifacts match the current view.</EmptyState>;
  return (
    <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>Artifact</th>
            <th>Type</th>
            <th>Release</th>
            <th>Access</th>
            {!compact && <th>Classification</th>}
            <th>Size</th>
            <th>Created</th>
            <th aria-label="Preview" />
          </tr>
        </thead>
        <tbody>
          {artifacts.map((artifact) => (
            <tr
              key={artifact.id}
              onClick={() => onSelect(artifact)}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onSelect(artifact);
                }
              }}
              tabIndex={0}
            >
              <td>
                <strong>{artifact.filename}</strong>
                <span className="mono subdued">{artifact.id.slice(0, 13)}…</span>
              </td>
              <td><span className="artifact-media-type">{artifact.mediaType}</span></td>
              <td><StatusPill value={artifact.releaseState} /></td>
              <td>
                <StatusPill value={artifact.effectiveAccess.level ?? "denied"} />
                <span className="subdued">
                  {artifact.effectiveAccess.sources.length} authority sources
                </span>
              </td>
              {!compact && <td><span className="code-label">{artifact.classification}</span></td>}
              <td>{formatBytes(artifact.byteLength)}</td>
              <td>{formatDate(artifact.createdAt)}</td>
              <td>
                <button className="table-preview-action" onClick={() => onSelect(artifact)}>
                  <Eye size={14} /> {artifactPreviewLabel(artifact.mediaType)}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
