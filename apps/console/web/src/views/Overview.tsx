import { Metric, SectionHeader, StatusPill } from "../components/primitives";
import { TaskTable } from "../components/TaskTable";
import { ArtifactTable } from "../components/ArtifactTable";
import { AuditOverview } from "../audit/Overview";
import type { ArtifactSummary, InstallationSnapshot, TaskSummary } from "../types";

export function Overview({
  snapshot,
  onTask,
  onArtifact,
  canReadAudit,
}: {
  snapshot: InstallationSnapshot;
  canReadAudit: boolean;
  onTask: (task: TaskSummary) => void;
  onArtifact: (artifact: ArtifactSummary) => void;
}) {
  const activeTasks = snapshot.tasks.filter((task) => ["queued", "running", "waiting", "cancel_requested"].includes(task.state));
  const healthy = snapshot.services.filter((service) => service.state === "healthy").length;
  const snapshotTime = new Date(snapshot.installation.generatedAt).getTime();
  const expiring = snapshot.artifacts.filter((artifact) => artifact.retentionExpiresAt && new Date(artifact.retentionExpiresAt).getTime() < snapshotTime + 30 * 86400_000).length;
  return (
    <>
      <div className="metrics-grid">
        <Metric label="Active work" value={String(activeTasks.length)} detail={`${snapshot.tasks.filter((task) => task.state === "waiting").length} waiting`} />
        <Metric label="Services" value={`${healthy}/${snapshot.services.length}`} detail={healthy === snapshot.services.length ? "All healthy" : "Attention required"} />
        <Metric label="Artifacts" value={String(snapshot.artifacts.length)} detail={`${expiring} expire within 30 days`} />
        <Metric label="Agents" value={String(snapshot.agents.length)} detail={`${snapshot.agents.filter((agent) => agent.state === "running").length} active now`} />
      </div>
      <div className="overview-grid">
        <section className="panel panel-wide">
          <SectionHeader title="Active work" count={activeTasks.length} />
          <TaskTable tasks={activeTasks} onSelect={onTask} compact />
        </section>
        <section className="panel">
          <SectionHeader title="Platform health" />
          <div className="health-list">
            {snapshot.services.map((service) => (
              <div key={service.id} className="health-row">
                <div><strong>{service.name}</strong><span>{service.detail}</span></div>
                <div className="health-tail"><StatusPill value={service.state} />{service.latencyMs !== undefined && <span>{service.latencyMs} ms</span>}</div>
              </div>
            ))}
          </div>
        </section>
        <section className="panel panel-wide">
          <SectionHeader title="Recent artifacts" count={snapshot.artifacts.length} />
          <ArtifactTable artifacts={snapshot.artifacts.slice(0, 4)} onSelect={onArtifact} compact />
        </section>
        {canReadAudit && <AuditOverview />}
      </div>
    </>
  );
}
