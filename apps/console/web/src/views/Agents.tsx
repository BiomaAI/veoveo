import { Bot } from "lucide-react";
import { SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import type { InstallationSnapshot } from "../types";

export function AgentsView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return (
    <section className="panel full-panel">
      <SectionHeader title="Agents" count={snapshot.agents.length} />
      <div className="item-grid">
        {snapshot.agents.map((agent) => (
          <article className="item-card" key={agent.id}>
            <div className="item-card-head">
              <div className="object-icon"><Bot size={18} /></div>
              <StatusPill value={agent.state} />
            </div>
            <h3>{agent.name}</h3>
            <span className="mono subdued">{agent.id}</span>
            <dl>
              <div><dt>Profile</dt><dd>{agent.profile}</dd></div>
              <div><dt>Pending wakes</dt><dd>{agent.pendingWakes}</dd></div>
              <div><dt>Last episode</dt><dd>{formatDate(agent.lastEpisodeAt)}</dd></div>
            </dl>
            <footer>{agent.detail}</footer>
          </article>
        ))}
      </div>
    </section>
  );
}
