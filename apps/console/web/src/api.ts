import { demoSnapshot } from "./demo";
import type {
  CallToolResult,
  CancelTaskResult,
  CreateTaskResult,
  GetTaskPayloadResult,
  GetTaskResult,
  ReadResourceResult,
  TaskMetadata,
} from "@modelcontextprotocol/sdk/types.js";
import type {
  AppCatalog,
  AgentElicitation,
  AgentWakeReceipt,
  ArtifactAccessRequest,
  ArtifactAccessRequestPage,
  ArtifactAccessRequestState,
  InstallationSnapshot,
  ClusterSnapshot,
  ReleaseState,
  RecordingPlaybackManifest,
  ShareLinkCreated,
} from "./types";
import { authenticationRequired, redirectToLogin } from "./auth";

let csrfToken: string | undefined;

export async function loadSnapshot(signal?: AbortSignal): Promise<InstallationSnapshot> {
  if (import.meta.env.VITE_DEMO_DATA === "true") {
    await new Promise((resolve) => window.setTimeout(resolve, 120));
    return demoSnapshot;
  }
  const response = await fetch("/console/api/snapshot", {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    signal
  });
  csrfToken = response.headers.get("x-veoveo-csrf-token") ?? undefined;
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error("Your account is authenticated but is not authorized to open this Console.");
  }
  if (!response.ok) {
    throw new Error(`Console API returned ${response.status}`);
  }
  return response.json() as Promise<InstallationSnapshot>;
}

export async function loadCluster(signal?: AbortSignal): Promise<ClusterSnapshot> {
  const response = await fetch("/console/api/cluster", {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) csrfToken = rotatedToken;
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error("Cluster inventory is not permitted for this console session.");
  }
  if (!response.ok) throw new Error(`Cluster inventory returned ${response.status}`);
  return response.json() as Promise<ClusterSnapshot>;
}

export async function consoleMutation<T>(path: string, init: RequestInit): Promise<T> {
  if (!csrfToken) {
    throw new Error("Console session has not been initialized");
  }
  const headers = new Headers(init.headers);
  headers.set("Accept", "application/json");
  headers.set("Content-Type", "application/json");
  headers.set("X-Veoveo-CSRF-Token", csrfToken);
  const response = await fetch(`/console/api/${path.replace(/^\/+/, "")}`, {
    ...init,
    method: init.method ?? "POST",
    credentials: "same-origin",
    headers
  });
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error("This operation is not permitted by the active console scopes and policy.");
  }
  if (!response.ok) {
    let detail: string | undefined;
    try {
      detail = ((await response.json()) as { error?: string }).error;
    } catch {
      detail = undefined;
    }
    throw new Error(detail ?? `Console API returned ${response.status}`);
  }
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) csrfToken = rotatedToken;
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export async function logoutConsole(): Promise<void> {
  if (!csrfToken) {
    throw new Error("Console session has not been initialized");
  }
  const response = await fetch("/auth/logout", {
    method: "POST",
    credentials: "same-origin",
    headers: { "X-Veoveo-CSRF-Token": csrfToken },
    redirect: "manual"
  });
  if (!response.ok) {
    throw new Error(`Console logout returned ${response.status}`);
  }
  csrfToken = undefined;
  redirectToLogin();
}

export async function cancelTask(taskId: string): Promise<void> {
  await consoleMutation(`tasks/${encodeURIComponent(taskId)}/cancel`, {
    method: "POST",
    body: ""
  });
}

interface AgentWakeReceiptWire {
  request_id: string;
  wake_id: string;
  agent_id: string;
  work_context: string;
  accepted_at: string;
}

interface AgentElicitationWire {
  elicitation_id: string;
  message: string;
  requested_schema?: unknown;
  requested_at: string;
}

function agentWakeReceipt(wire: AgentWakeReceiptWire): AgentWakeReceipt {
  return {
    requestId: wire.request_id,
    wakeId: wire.wake_id,
    agentId: wire.agent_id,
    workContext: wire.work_context,
    acceptedAt: wire.accepted_at,
  };
}

export async function sendAgentMessage(
  agentId: string,
  requestId: string,
  message: string,
): Promise<AgentWakeReceipt> {
  const wire = await consoleMutation<AgentWakeReceiptWire>(
    `agents/${encodeURIComponent(agentId)}/messages`,
    {
      method: "POST",
      body: JSON.stringify({ request_id: requestId, message }),
    },
  );
  return agentWakeReceipt(wire);
}

export async function loadAgentElicitations(
  agentId: string,
  signal?: AbortSignal,
): Promise<AgentElicitation[]> {
  const response = await fetch(
    `/console/api/agents/${encodeURIComponent(agentId)}/elicitations`,
    {
      credentials: "same-origin",
      headers: { Accept: "application/json" },
      signal,
    },
  );
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) csrfToken = rotatedToken;
  if (response.status === 401) authenticationRequired();
  if (response.status === 403) {
    throw new Error("Agent elicitations are not permitted for this Console session.");
  }
  if (!response.ok) throw new Error(`Agent elicitations returned ${response.status}`);
  const values = (await response.json()) as AgentElicitationWire[];
  return values.map((wire) => ({
    elicitationId: wire.elicitation_id,
    message: wire.message,
    requestedSchema: wire.requested_schema,
    requestedAt: wire.requested_at,
  }));
}

export async function decideAgentElicitation(
  agentId: string,
  elicitationId: string,
  requestId: string,
  decision: { action: "accept"; content: Record<string, unknown> } | { action: "decline" | "cancel" },
): Promise<AgentWakeReceipt> {
  const wire = await consoleMutation<AgentWakeReceiptWire>(
    `agents/${encodeURIComponent(agentId)}/elicitations/${encodeURIComponent(elicitationId)}/decision`,
    {
      method: "POST",
      body: JSON.stringify({ request_id: requestId, ...decision }),
    },
  );
  return agentWakeReceipt(wire);
}

export async function setArtifactReleaseState(artifactId: string, releaseState: ReleaseState): Promise<void> {
  await consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/release-state`, {
    method: "PUT",
    body: JSON.stringify({ release_state: releaseState })
  });
}

export async function grantArtifact(
  artifactId: string,
  subject: { kind: "principal" | "group"; id: string },
  level: "read" | "write" | "admin"
): Promise<void> {
  await consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/grants`, {
    method: "POST",
    body: JSON.stringify({ subject, level })
  });
}

export async function revokeArtifactGrant(
  artifactId: string,
  subject: { kind: "principal" | "group"; id: string }
): Promise<void> {
  await consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/grants`, {
    method: "DELETE",
    body: JSON.stringify(subject)
  });
}

interface ArtifactAccessRequestWire {
  id: string;
  artifact_id: string;
  work_context: string;
  requester: string;
  requested_level: "read" | "write" | "admin";
  justification: string;
  state: ArtifactAccessRequestState;
  decided_by?: string;
  decision_note?: string;
  created_at: string;
  updated_at: string;
  decided_at?: string;
}

interface ArtifactAccessRequestPageWire {
  requests: ArtifactAccessRequestWire[];
  next_cursor?: string;
}

function artifactAccessRequest(wire: ArtifactAccessRequestWire): ArtifactAccessRequest {
  return {
    id: wire.id,
    artifactId: wire.artifact_id,
    workContext: wire.work_context,
    requester: wire.requester,
    requestedLevel: wire.requested_level,
    justification: wire.justification,
    state: wire.state,
    decidedBy: wire.decided_by,
    decisionNote: wire.decision_note,
    createdAt: wire.created_at,
    updatedAt: wire.updated_at,
    decidedAt: wire.decided_at,
  };
}

export async function loadArtifactAccessRequests(
  scope: "mine" | "reviewable",
  state?: ArtifactAccessRequestState,
  signal?: AbortSignal
): Promise<ArtifactAccessRequestPage> {
  const query = new URLSearchParams({ scope, limit: "50" });
  if (state) query.set("state", state);
  const response = await fetch(`/console/api/artifact-access-requests?${query}`, {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) csrfToken = rotatedToken;
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error("Access requests are not available to the active Work Context membership.");
  }
  if (!response.ok) throw new Error(`Access requests returned ${response.status}`);
  const page = (await response.json()) as ArtifactAccessRequestPageWire;
  return {
    requests: page.requests.map(artifactAccessRequest),
    nextCursor: page.next_cursor,
  };
}

export async function requestArtifactAccess(
  artifactId: string,
  requestedLevel: "read" | "write" | "admin",
  justification: string
): Promise<ArtifactAccessRequest> {
  const wire = await consoleMutation<ArtifactAccessRequestWire>(
    `artifacts/${encodeURIComponent(artifactId)}/access-requests`,
    {
      method: "POST",
      body: JSON.stringify({
        requested_level: requestedLevel,
        justification,
      }),
    }
  );
  return artifactAccessRequest(wire);
}

export async function decideArtifactAccessRequest(
  requestId: string,
  decision: "approve" | "deny",
  note?: string
): Promise<ArtifactAccessRequest> {
  const wire = await consoleMutation<ArtifactAccessRequestWire>(
    `artifact-access-requests/${encodeURIComponent(requestId)}/decision`,
    {
      method: "POST",
      body: JSON.stringify({ decision, ...(note ? { note } : {}) }),
    }
  );
  return artifactAccessRequest(wire);
}

export async function cancelArtifactAccessRequest(
  requestId: string
): Promise<ArtifactAccessRequest> {
  const wire = await consoleMutation<ArtifactAccessRequestWire>(
    `artifact-access-requests/${encodeURIComponent(requestId)}/cancel`,
    { method: "POST", body: "" }
  );
  return artifactAccessRequest(wire);
}

export async function createArtifactShareLink(
  artifactId: string,
  expiresAt: string,
  maxDownloads?: number
): Promise<ShareLinkCreated> {
  return consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/share-links`, {
    method: "POST",
    body: JSON.stringify({
      expires_at: expiresAt,
      ...(maxDownloads ? { max_downloads: maxDownloads } : {})
    })
  });
}

export async function revokeArtifactShareLink(artifactId: string, linkId: string): Promise<void> {
  await consoleMutation(
    `artifacts/${encodeURIComponent(artifactId)}/share-links/${encodeURIComponent(linkId)}`,
    { method: "DELETE", body: "" }
  );
}

export function artifactDownloadUrl(artifactId: string): string {
  return `/console/api/artifacts/${encodeURIComponent(artifactId)}/download`;
}

export function artifactPreviewUrl(artifactId: string): string {
  return `/console/api/artifacts/${encodeURIComponent(artifactId)}/preview`;
}

export async function loadRecordingPlayback(
  recordingId: string,
  options: { signal?: AbortSignal; playbackSession?: string } = {}
): Promise<RecordingPlaybackManifest> {
  const headers: Record<string, string> = { Accept: "application/json" };
  if (options.playbackSession) {
    headers["X-Veoveo-Playback-Session"] = options.playbackSession;
  }
  const response = await fetch(
    `/console/api/recordings/${encodeURIComponent(recordingId)}/playback`,
    {
      credentials: "same-origin",
      headers,
      signal: options.signal,
    }
  );
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error("Playback is not permitted by the active recording policy.");
  }
  if (!response.ok) {
    throw new Error(`Recording playback returned ${response.status}`);
  }
  return response.json() as Promise<RecordingPlaybackManifest>;
}

export function recordingLiveRrdStreamRoute(recordingId: string): string {
  const path = `/console/api/recordings/${encodeURIComponent(recordingId)}/live/rrd-stream`;
  return new URL(path, window.location.origin).toString();
}

export function recordingBlueprintUrl(recordingId: string, revision: number): string {
  const path = `/console/api/recordings/${encodeURIComponent(recordingId)}/blueprints/${encodeURIComponent(revision)}/data.rrd`;
  return new URL(path, window.location.origin).toString();
}

export async function loadApps(signal?: AbortSignal): Promise<AppCatalog> {
  const response = await fetch("/console/api/apps", {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) csrfToken = rotatedToken;
  if (response.status === 401) {
    authenticationRequired();
  }
  if (!response.ok) throw new Error(`App catalog returned ${response.status}`);
  return response.json() as Promise<AppCatalog>;
}

export function appFrameUrl(resourceUri: string): string {
  return `/console/api/apps/frame?uri=${encodeURIComponent(resourceUri)}`;
}

export async function callAppTool(
  server: string,
  appUri: string,
  tool: string,
  toolArguments: Record<string, unknown>
): Promise<CallToolResult>;
export async function callAppTool(
  server: string,
  appUri: string,
  tool: string,
  toolArguments: Record<string, unknown>,
  task: TaskMetadata
): Promise<CreateTaskResult>;
export async function callAppTool(
  server: string,
  appUri: string,
  tool: string,
  toolArguments: Record<string, unknown>,
  task?: TaskMetadata
): Promise<CallToolResult | CreateTaskResult> {
  return consoleMutation<CallToolResult | CreateTaskResult>("apps/call", {
    method: "POST",
    body: JSON.stringify({ server, appUri, tool, arguments: toolArguments, ...(task ? { task } : {}) }),
  });
}

export async function getAppTask(
  server: string,
  appUri: string,
  taskId: string
): Promise<GetTaskResult> {
  return consoleMutation<GetTaskResult>("apps/task/get", {
    method: "POST",
    body: JSON.stringify({ server, appUri, taskId }),
  });
}

export async function getAppTaskResult(
  server: string,
  appUri: string,
  taskId: string
): Promise<GetTaskPayloadResult> {
  return consoleMutation<GetTaskPayloadResult>("apps/task/result", {
    method: "POST",
    body: JSON.stringify({ server, appUri, taskId }),
  });
}

export async function cancelAppTask(
  server: string,
  appUri: string,
  taskId: string
): Promise<CancelTaskResult> {
  return consoleMutation<CancelTaskResult>("apps/task/cancel", {
    method: "POST",
    body: JSON.stringify({ server, appUri, taskId }),
  });
}

export async function readAppResource(
  server: string,
  appUri: string,
  uri: string
): Promise<ReadResourceResult> {
  return consoleMutation<ReadResourceResult>("apps/read", {
    method: "POST",
    body: JSON.stringify({ server, appUri, uri }),
  });
}

export interface AppResourceEventSubscription {
  subscriptionId: string;
  uri: string;
}

export async function openAppResourceEvents(
  server: string,
  appUri: string,
  subscriptions: AppResourceEventSubscription[],
  signal?: AbortSignal | null,
): Promise<Response> {
  if (!csrfToken) throw new Error("Console session has not been initialized");
  const response = await fetch("/console/api/apps/resource-events", {
    method: "POST",
    credentials: "same-origin",
    headers: {
      Accept: "text/event-stream",
      "Content-Type": "application/json",
      "X-Veoveo-CSRF-Token": csrfToken,
    },
    body: JSON.stringify({ server, appUri, subscriptions }),
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) csrfToken = rotatedToken;
  if (response.status === 401) authenticationRequired();
  if (response.status === 403) {
    throw new Error("This App resource subscription is not permitted by the active Console policy.");
  }
  return response;
}

export async function unsubscribeAppResource(subscriptionId: string): Promise<void> {
  await consoleMutation<Record<string, never>>("apps/resource-unsubscribe", {
    method: "POST",
    body: JSON.stringify({ subscriptionId }),
  });
}
