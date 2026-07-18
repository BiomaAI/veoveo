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
    productLabel: string;
    logo?: string;
    accentColor?: string;
    version: string;
    offlineMode: boolean;
    generatedAt: string;
  };
  session: {
    displayName: string;
    principalId: string;
    tenantId: string;
    tenantName: string;
    availableTenants: Array<{ id: string; name: string }>;
  };
  stream: {
    cursor: string;
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
  recording?: {
    recordingId: string;
    kind: string;
    segmentId?: string;
    ordinal?: number;
  };
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
  state: "live" | "ready" | "sealing" | "sealed" | "interrupted" | "failed";
  segmentCount: number;
  playableSegmentCount: number;
  playableByteLength: number;
  startedAt: string;
  lastDataAt: string;
  endedAt?: string;
  sealedAt?: string;
}

export interface RecordingPlaybackManifest {
  recording_id: string;
  application_id: string;
  recording_key: string;
  state: RecordingSummary["state"];
  started_at: string;
  ended_at?: string;
  playback_ticket: string;
  segments: Array<{
    segment_id: string;
    ordinal: number;
    byte_len: number;
    sha256: string;
  }>;
  live_segment?: {
    segment_id: string;
    ordinal: number;
    byte_len: number;
  };
}

export interface McpServerSummary {
  id: string;
  name: string;
  uriScheme: string;
  transport: "streamable_http" | "sse" | "stdio";
  endpoint: string;
  state: HealthState;
  checkedAt: string;
  capabilities: {
    tools: boolean;
    resources: boolean;
    resourceTemplates: boolean;
    resourceSubscriptions: boolean;
    prompts: boolean;
    completions: boolean;
    tasks: boolean;
    notifications: boolean;
  };
  tools: string[];
  compatibilityHelpers: string[];
  resources: string[];
  prompts: string[];
  requiredScopes: string[];
  ownedRoutes: Array<{ path: string; purpose: string }>;
  profiles: string[];
}

export interface ClusterSnapshot {
  orchestrator: "Kubernetes";
  namespace: string;
  generatedAt: string;
  workloads: ClusterWorkload[];
  pods: ClusterPod[];
  services: ClusterService[];
  storage: ClusterStorage[];
  ingresses: Array<{ name: string; className?: string; hosts: string[] }>;
  networkPolicies: string[];
  disruptionBudgets: string[];
  configMaps: string[];
}

export interface ClusterWorkload {
  name: string;
  kind: "Deployment" | "StatefulSet" | "Job";
  desired: number;
  ready: number;
  available: number;
  images: string[];
  createdAt?: string;
}

export interface ClusterPod {
  name: string;
  component?: string;
  phase: string;
  ready: number;
  containers: number;
  restarts: number;
  node?: string;
  images: string[];
}

export interface ClusterService {
  name: string;
  kind: string;
  clusterIp?: string;
  ports: string[];
}

export interface ClusterStorage {
  name: string;
  phase: string;
  requested?: string;
  capacity?: string;
  storageClass?: string;
  accessModes: string[];
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

export interface AppToolDescriptor {
  name: string;
  title?: string;
  description?: string;
  inputSchema: Record<string, unknown>;
}

export interface AppDescriptor {
  server: string;
  resourceUri: string;
  name: string;
  title?: string;
  description?: string;
  icons?: string[];
  tools: AppToolDescriptor[];
}

export interface AppCatalog {
  apps: AppDescriptor[];
}
