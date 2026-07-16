import { EmptyState, RowLink, StatusPill } from "./primitives";
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
            <th>Release</th>
            <th>Access</th>
            {!compact && <th>Classification</th>}
            <th>Size</th>
            <th>Created</th>
            <th aria-label="Open" />
          </tr>
        </thead>
        <tbody>
          {artifacts.map((artifact) => (
            <tr key={artifact.id} onClick={() => onSelect(artifact)} tabIndex={0}>
              <td>
                <strong>{artifact.filename}</strong>
                <span className="mono subdued">{artifact.id.slice(0, 13)}…</span>
              </td>
              <td><StatusPill value={artifact.releaseState} /></td>
              <td>
                <span>{artifact.authorizedGrants} grants</span>
                <span className="subdued">{artifact.activeLinks} active links</span>
              </td>
              {!compact && <td><span className="code-label">{artifact.classification}</span></td>}
              <td>{formatBytes(artifact.byteLength)}</td>
              <td>{formatDate(artifact.createdAt)}</td>
              <td><RowLink /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
