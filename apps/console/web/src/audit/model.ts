import type { AuditClass, AuditDetail, AuditOutcome, AuditPartition, AuditQuery, AuditTarget } from "../generated/audit.ts";

export const classes = ["api_activity", "authentication", "account_change", "artifact_activity", "live_view_access", "computer_activity"] satisfies AuditClass[];
export const outcomes = ["allowed", "denied", "succeeded", "failed"] satisfies AuditOutcome[];
export const words = (value: string): string => value.replaceAll("_", " ");
export function partitionLabel(partition: AuditPartition): string {
  return partition.kind === "installation" ? "Installation" : `Tenant ${partition.tenant}`;
}
export function activityLabel(detail: AuditDetail): string {
  if (detail.kind === "read") return words(detail.method);
  if ("operation" in detail) return `${words(detail.operation)} · ${words(detail.kind)}`;
  if ("activity" in detail) return `${words(detail.kind)} · ${words(detail.activity)}`;
  return words(detail.kind);
}
export function targetLabel(target: AuditTarget): string {
  switch (target.kind) {
    case "platform_resource": case "resource": case "resource_template": return target.uri;
    case "artifact": return `Artifact ${target.artifact}`;
    case "computer": return `Computer ${target.computer}`;
    case "task": return `Task ${target.task}`;
    case "task_route": return `${target.server} / ${target.route}`;
    case "server": return target.server;
    case "principal": return target.principal;
    case "work_context": return target.context;
    case "tool": return `${target.server} / ${target.tool}`;
    case "prompt": return `${target.server} / ${target.prompt}`;
    case "discovery": return `${target.server ?? "All servers"} / ${words(target.collection)}`;
    case "profile": return `Profile ${target.profile}`;
    case "client": return `Client ${target.client}`;
    case "audit_log": return `Audit / ${partitionLabel(target.partition)}`;
    case "installation": return "Installation";
    default: { const exhaustive: never = target; return exhaustive; }
  }
}
export function initialQuery(partition: AuditPartition): AuditQuery {
  return { partition, order: "newest_first", cursor: null, class: null, actor: null,
    target: null, outcome: null, trace: null, from: null, until: null, limit: 50 };
}
