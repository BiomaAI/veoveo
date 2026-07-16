import { SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import type { InstallationSnapshot } from "../types";

export function AccessView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return (
    <section className="panel full-panel">
      <SectionHeader title="Active policy sets" count={snapshot.policies.length} />
      <p className="panel-intro">
        Access policy is part of the versioned gateway control plane. Changes are validated and activated as one atomic
        revision; this console reports the active policy sets and does not edit them independently.
      </p>
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              <th>Policy</th>
              <th>State</th>
              <th>Revision</th>
              <th>Rules</th>
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {snapshot.policies.map((policy) => (
              <tr key={policy.id}>
                <td>
                  <strong>{policy.name}</strong>
                  <span className="mono subdued">{policy.id}</span>
                </td>
                <td><StatusPill value={policy.state} /></td>
                <td>r{policy.revision}</td>
                <td>{policy.rules}</td>
                <td>{formatDate(policy.updatedAt)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
