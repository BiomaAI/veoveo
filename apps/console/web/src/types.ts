import type { AgentWakeReceipt as WakeReceiptWire, AgentInputRequestView, AgentConversationEntry as ConversationEntryWire, AgentConversationView } from "./generated/agent-control";
import type { ArtifactAccessRequest as AccessRequestWire, ArtifactAccessRequestPage as AccessRequestPageWire } from "./generated/artifact-transfer";
export type { ArtifactAccessRequestState, ArtifactShareLink as ShareLinkCreated } from "./generated/artifact-transfer";

// Browser presentation uses camelCase while these owner HTTP contracts use snake_case.
type CamelKey<Key extends string> = Key extends `${infer Head}_${infer Tail}`
  ? `${Head}${Capitalize<CamelKey<Tail>>}` : Key;
type Presentation<Model> = {
  [Key in keyof Model as Key extends string ? CamelKey<Key> : Key]: Model[Key];
};

import type { ConsoleInstallation, ConsoleSession, InvocationMode, WorkContextMembershipLevel } from "./generated/console";
export type { InvocationMode } from "./generated/console";
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
  installation: ConsoleInstallation;
  session: ConsoleSession;
  principals: PrincipalSummary[];
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
}

export interface PrincipalSummary {
  id: string;
  displayName: string;
}

export type WorkContextMembership = WorkContextMembershipLevel;

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
  byteLength: number | null;
  owner: string;
  outputOwner: {
    kind: "principal" | "group";
    id: string;
  };
  provenance: {
    workContext: string;
    producer: string;
    invocationMode: InvocationMode;
    initiator?: string;
    delegationId?: string;
    policyRevision: string;
  };
  effectiveAccess: {
    level?: "read" | "write" | "admin";
    read: boolean;
    write: boolean;
    admin: boolean;
    clearanceSatisfied: boolean;
    requestable: boolean;
    denialReason?: "tenant_boundary" | "clearance" | "need_to_know";
    sources: Array<{
      kind: "principal_grant" | "group_grant" | "work_context";
      subject: string;
      level: "read" | "write" | "admin";
    }>;
  };
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
    layerId?: string;
    ordinal?: number;
  };
}

export type ArtifactAccessRequest = Presentation<AccessRequestWire>;
export type ArtifactAccessRequestPage = Omit<Presentation<AccessRequestPageWire>, "requests"> & {
  requests: ArtifactAccessRequest[];
};

export interface ArtifactGrantSummary {
  subjectKind: "principal" | "group";
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

export interface AgentSummary {
  id: string;
  name: string;
  profile: string;
  state: "idle" | "running" | "waiting" | "disabled" | "failed";
  runnerLeaseExpiresAt?: string;
  pendingWakes: number;
  lastEpisodeAt?: string;
  detail: string;
}

export type AgentWakeReceipt = Presentation<WakeReceiptWire>;
export type AgentConversationEntry = Omit<Presentation<ConversationEntryWire>, "inReplyToRequestIds"> & {
  inReplyToRequestIds: NonNullable<ConversationEntryWire["in_reply_to_request_ids"]>;
};
export type AgentConversation = Omit<Presentation<AgentConversationView>, "entries"> & {
  entries: AgentConversationEntry[];
};
export type AgentInputRequest = Presentation<AgentInputRequestView>;

export interface RecordingSummary {
  id: string;
  application: string;
  recordingKey: string;
  state: "live" | "ready" | "sealing" | "sealed" | "interrupted" | "failed";
  layerCount: number;
  committedLayerCount: number;
  committedByteLength: number;
  startedAt: string;
  lastDataAt: string;
  endedAt?: string;
  sealedAt?: string;
}

export type { PlaybackManifest as RecordingPlaybackManifest } from "./generated/recording-playback";

export interface McpServerSummary {
  id: string;
  name: string;
  uriScheme: string;
  transport: "streamable_http";
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
    toolsListChanged: boolean;
    promptsListChanged: boolean;
    resourcesListChanged: boolean;
  };
  tools: string[];
  compatibilityHelpers: string[];
  resources: string[];
  prompts: string[];
  requiredScopes: string[];
  ownedRoutes: Array<{ path: string; purpose: string }>;
  profiles: string[];
}

export type { ClusterSnapshot, ClusterWorkload, ClusterPod, ClusterService, ClusterStorage } from "./generated/cluster";

export interface PolicySummary {
  id: string;
  name: string;
  revision: number;
  state: "draft" | "active" | "retired";
  rules: number;
  updatedAt: string;
}



export type { AppToolDescriptor, AppDescriptor, AppResourceDependency, AppToolDependency, AppCatalog, GatewayDiscoveryFailure as AppCatalogDegradation } from "./generated/app-catalog";
