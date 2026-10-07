import type { AgentWakeReceipt as WakeReceiptWire, AgentInputRequestView, AgentConversationEntry as ConversationEntryWire, AgentConversationView } from "./generated/agent-control";
import type { ArtifactAccessRequest as AccessRequestWire, ArtifactAccessRequestPage as AccessRequestPageWire } from "./generated/artifact-transfer";
export type { ArtifactAccessRequestState, ArtifactShareLink as ShareLinkCreated } from "./generated/artifact-transfer";

// Browser presentation uses camelCase while these owner HTTP contracts use snake_case.
type CamelKey<Key extends string> = Key extends `${infer Head}_${infer Tail}`
  ? `${Head}${Capitalize<CamelKey<Tail>>}` : Key;
type Presentation<Model> = {
  [Key in keyof Model as Key extends string ? CamelKey<Key> : Key]: Model[Key];
};

import type { WorkContextMembershipLevel } from "./generated/console";
export type { InvocationMode } from "./generated/console";
export type { PrincipalSummary, TaskSummary, ArtifactSummary, ArtifactGrantSummaryValue as ArtifactGrantSummary, ArtifactShareLinkSummary, AgentSummary, RecordingSummary, ServerSummary as McpServerSummary, PolicySummary, GatewayServerHealthState as HealthState, TaskStatus as TaskState, ArtifactReleaseState as ReleaseState } from "./generated/console";

export type WorkContextMembership = WorkContextMembershipLevel;

export type ArtifactAccessRequest = Presentation<AccessRequestWire>;
export type ArtifactAccessRequestPage = Omit<Presentation<AccessRequestPageWire>, "requests"> & {
  requests: ArtifactAccessRequest[];
};

export type AgentWakeReceipt = WakeReceiptWire;
export type AgentConversationEntry = Omit<ConversationEntryWire, "inReplyToRequestIds"> & {
  inReplyToRequestIds: NonNullable<ConversationEntryWire["inReplyToRequestIds"]>;
};
export type AgentConversation = Omit<AgentConversationView, "entries"> & {
  entries: AgentConversationEntry[];
};
export type AgentInputRequest = AgentInputRequestView;

export type { PlaybackManifest as RecordingPlaybackManifest } from "./generated/recording-playback";

export type { ClusterSnapshot, ClusterWorkload, ClusterPod, ClusterService, ClusterStorage } from "./generated/cluster";

export type { AppToolDescriptor, AppDescriptor, AppResourceDependency, AppToolDependency, AppCatalog, GatewayDiscoveryFailure as AppCatalogDegradation } from "./generated/app-catalog";

// Synthetic demo latency is browser presentation data, outside the snapshot wire contract.
import type { ConsoleSnapshot, ServiceSummary } from "./generated/console";
export type ServiceHealth = ServiceSummary & { latencyMs?: number };
export type InstallationSnapshot = Omit<ConsoleSnapshot, "services"> & { services: ServiceHealth[] };
