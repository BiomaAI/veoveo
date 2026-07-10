export type HealthState = "healthy" | "degraded" | "offline";
export type TaskState =
  | "queued"
  | "running"
  | "waiting"
  | "succeeded"
  | "failed"
  | "cancel_requested"
  | "cancelled";
export type ReleaseState = "private" | "releasable" | "released";

export interface InstallationSnapshot {
  installation: {
    name: string;
    version: string;
    offlineMode: boolean;
    databaseTopology: "single-node";
    generatedAt: string;
  };
  session: {
    displayName: string;
    principalId: string;
    tenantId: string;
    tenantName: string;
    availableTenants: Array<{ id: string; name: string }>;
  };
  services: ServiceHealth[];
  tasks: TaskSummary[];
  artifacts: ArtifactSummary[];
  agents: AgentSummary[];
  recordings: RecordingSummary[];
  servers: McpServerSummary[];
  policies: PolicySummary[];
  audit: AuditSummary[];
}

export interface ServiceHealth {
  id: string;
  name: string;
  kind: "database" | "gateway" | "mcp" | "object_store" | "observability";
  state: HealthState;
  detail: string;
  latencyMs?: number;
  checkedAt: string;
}

export interface TaskSummary {
  id: string;
  type: string;
  server: string;
  owner: string;
  state: TaskState;
  recoveryClass: "resume" | "webhook_wait" | "interrupted_indeterminate";
  progress: number;
  createdAt: string;
  updatedAt: string;
  resultArtifactId?: string;
  message?: string;
}

export interface ArtifactSummary {
  id: string;
  filename: string;
  mediaType: string;
  byteLength: number;
  owner: string;
  taskId?: string;
  classification: string;
  labels: string[];
  releaseState: ReleaseState;
  authorizedGrants: number;
  activeLinks: number;
  grants: ArtifactGrantSummary[];
  shareLinks: ArtifactShareLinkSummary[];
  retentionExpiresAt?: string;
  createdAt: string;
}

export interface ArtifactGrantSummary {
  subjectKind: "user" | "group";
  subject: string;
  permission: "read" | "write" | "admin";
  labels: string[];
  expiresAt?: string;
  createdAt: string;
}

export interface ArtifactShareLinkSummary {
  id: string;
  permission: "read" | "write" | "admin";
  expiresAt: string;
  maxDownloads?: number;
  downloadCount: number;
  revokedAt?: string;
  createdAt: string;
  active: boolean;
}

export interface ShareLinkCreated {
  link_id: string;
  artifact_id: string;
  url: string;
  expires_at: string;
  max_downloads?: number;
}

export interface AgentSummary {
  id: string;
  name: string;
  profile: string;
  state: "idle" | "running" | "waiting" | "disabled" | "failed";
  pendingWakes: number;
  lastEpisodeAt?: string;
  detail: string;
}

export interface RecordingSummary {
  id: string;
  application: string;
  recordingKey: string;
  state: "open" | "sealed" | "failed";
  segments: number;
  byteLength: number;
  startedAt: string;
  endedAt?: string;
}

export interface McpServerSummary {
  id: string;
  name: string;
  transport: "streamable_http" | "sse" | "stdio";
  endpoint: string;
  state: HealthState;
  tools: number;
  resources: number;
  prompts: number;
  profiles: string[];
}

export interface PolicySummary {
  id: string;
  name: string;
  revision: number;
  state: "draft" | "active" | "retired";
  rules: number;
  updatedAt: string;
}

export interface AuditSummary {
  id: string;
  occurredAt: string;
  actor: string;
  action: string;
  resource: string;
  outcome: "allowed" | "denied" | "failed";
  sourceIp?: string;
  traceId?: string;
}
