import { useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { EmptyState, Metric, SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import { queryKeys, useCluster } from "../queries";
import type { InstallationSnapshot } from "../types";

export function ClusterView({ snapshot }: { snapshot: InstallationSnapshot }) {
  const queryClient = useQueryClient();
  const { data: cluster, error, isLoading, isFetching } = useCluster();
  const refresh = () => void queryClient.invalidateQueries({ queryKey: queryKeys.cluster });
  if (isLoading) return <section className="panel full-panel"><EmptyState>Loading Kubernetes inventory…</EmptyState></section>;
  if (!cluster) {
    const message = error instanceof Error ? error.message : "Cluster inventory is unavailable.";
    return <section className="panel full-panel"><SectionHeader title="Cluster" actions={<button className="button button-secondary" onClick={refresh}><RefreshCw size={14} /> Retry</button>} /><EmptyState>{message}</EmptyState></section>;
  }
  const readyWorkloads = cluster.workloads.filter((workload) => workload.ready >= workload.desired).length;
  const readyPods = cluster.pods.filter((pod) => pod.phase === "Running" && pod.ready === pod.containers).length;
  return <div className="cluster-layout">
    {error instanceof Error && <div className="action-error">{error.message}</div>}
    <div className="metrics-grid cluster-metrics">
      <Metric label="Workloads" value={`${readyWorkloads}/${cluster.workloads.length}`} detail="Ready in namespace" />
      <Metric label="Pods" value={`${readyPods}/${cluster.pods.filter((pod) => pod.phase === "Running").length}`} detail={`${cluster.pods.reduce((sum, pod) => sum + pod.restarts, 0)} total restarts`} />
      <Metric label="Services" value={String(cluster.services.length)} detail={`${cluster.ingresses.length} ingress resources`} />
      <Metric label="Storage" value={String(cluster.storage.length)} detail={`${cluster.storage.filter((claim) => claim.phase === "Bound").length} claims bound`} />
    </div>
    <section className="panel full-panel"><SectionHeader title="Kubernetes workloads" count={cluster.workloads.length} actions={<button className="button button-secondary" onClick={refresh} disabled={isFetching}><RefreshCw size={14} className={isFetching ? "spin" : ""} /> Refresh</button>} /><p className="panel-intro">{cluster.orchestrator} namespace <code>{cluster.namespace}</code> · Veoveo {snapshot.installation.version} · {snapshot.installation.offlineMode ? "air-gapped" : "connected"}</p><div className="table-scroll"><table><thead><tr><th>Workload</th><th>Kind</th><th>Ready</th><th>Available</th><th>Image</th><th>Created</th></tr></thead><tbody>{cluster.workloads.map((workload) => <tr key={`${workload.kind}:${workload.name}`}><td><strong>{workload.name}</strong></td><td><span className="code-label">{workload.kind}</span></td><td><StatusPill value={workload.ready >= workload.desired ? "healthy" : "degraded"} /><span className="subdued">{workload.ready}/{workload.desired}</span></td><td>{workload.available}</td><td className="mono image-cell">{workload.images.join(", ")}</td><td>{formatDate(workload.createdAt)}</td></tr>)}</tbody></table></div></section>
    <section className="panel full-panel"><SectionHeader title="Pods" count={cluster.pods.length} /><div className="table-scroll"><table><thead><tr><th>Pod</th><th>Phase</th><th>Ready</th><th>Restarts</th><th>Node</th><th>Image</th></tr></thead><tbody>{cluster.pods.map((pod) => <tr key={pod.name}><td><strong>{pod.component ?? pod.name}</strong><span className="mono subdued">{pod.name}</span></td><td><StatusPill value={pod.phase.toLowerCase()} /></td><td>{pod.ready}/{pod.containers}</td><td>{pod.restarts}</td><td className="mono">{pod.node ?? "-"}</td><td className="mono image-cell">{pod.images.join(", ")}</td></tr>)}</tbody></table></div></section>
    <section className="panel"><SectionHeader title="Persistent storage" count={cluster.storage.length} /><div className="cluster-card-list">{cluster.storage.map((claim) => <article key={claim.name}><div><strong>{claim.name}</strong><span>{claim.storageClass ?? "default storage class"}</span></div><StatusPill value={claim.phase.toLowerCase()} /><dl><div><dt>Requested</dt><dd>{claim.requested ?? "-"}</dd></div><div><dt>Capacity</dt><dd>{claim.capacity ?? "-"}</dd></div><div><dt>Access</dt><dd>{claim.accessModes.join(", ")}</dd></div></dl></article>)}</div></section>
    <section className="panel"><SectionHeader title="Network" /><div className="cluster-network"><div><strong>Ingress</strong>{cluster.ingresses.length ? cluster.ingresses.map((ingress) => <span key={ingress.name}><code>{ingress.name}</code> · {ingress.hosts.join(", ") || "no host"}</span>) : <span>No ingress resources</span>}</div><div><strong>Services</strong><span>{cluster.services.length} service objects</span></div><div><strong>Network policies</strong><span>{cluster.networkPolicies.length} active policies</span></div><div><strong>Disruption budgets</strong><span>{cluster.disruptionBudgets.length} budgets</span></div><div><strong>Configuration</strong><span>{cluster.configMaps.length} ConfigMaps</span></div></div></section>
    <section className="panel full-panel"><SectionHeader title="Services" count={cluster.services.length} /><div className="table-scroll"><table><thead><tr><th>Service</th><th>Type</th><th>Cluster IP</th><th>Ports</th></tr></thead><tbody>{cluster.services.map((service) => <tr key={service.name}><td><strong>{service.name}</strong></td><td><span className="code-label">{service.kind}</span></td><td className="mono">{service.clusterIp ?? "-"}</td><td className="mono">{service.ports.join(", ")}</td></tr>)}</tbody></table></div></section>
  </div>;
}
