import { SectionHeader, StatusPill } from "../components/primitives";
import { formatBytes, formatDate } from "../format";
import type { InstallationSnapshot } from "../types";

export function RecordingsView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return (
    <section className="panel full-panel">
      <SectionHeader title="Recordings" count={snapshot.recordings.length} />
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              <th>Recording</th>
              <th>State</th>
              <th>Application</th>
              <th>Segments</th>
              <th>Size</th>
              <th>Started</th>
              <th>Ended</th>
            </tr>
          </thead>
          <tbody>
            {snapshot.recordings.map((recording) => (
              <tr key={recording.id}>
                <td>
                  <strong>{recording.recordingKey}</strong>
                  <span className="mono subdued">{recording.id.slice(0, 13)}…</span>
                </td>
                <td><StatusPill value={recording.state} /></td>
                <td>{recording.application}</td>
                <td>{recording.segments}</td>
                <td>{formatBytes(recording.byteLength)}</td>
                <td>{formatDate(recording.startedAt)}</td>
                <td>{formatDate(recording.endedAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
