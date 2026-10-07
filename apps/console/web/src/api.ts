import { recordingPlaybackValue } from "./recordingPlayback";
import type { AgentWakeReceipt as AgentWakeReceiptWire } from "./generated/agent-control";
import type { ArtifactAccessRequest as ArtifactAccessRequestWire, ArtifactAccessRequestPage as ArtifactAccessRequestPageWire } from "./generated/artifact-transfer";
import { parseAgentControl, parseConsole } from "./generatedContracts";
import { demoSnapshot } from "./demo";
import { agentInputRequestDecisionPath, agentInputRequestsApiPath } from "./agentControl";
import type {
  AppReadResourceResult,
  AppToolRequestExtras,
  AppToolResult,
  InputResponses,
  TaskAckResult,
  TaskDetailResult,
} from "./apps/protocol";
import type {
  AppCatalog,
  AgentConversation,
  AgentInputRequest,
  AgentWakeReceipt,
  ArtifactAccessRequest,
  ArtifactSummary,
  ArtifactAccessRequestPage,
  ArtifactAccessRequestState,
  InstallationSnapshot,
  ClusterSnapshot,
  ReleaseState,
  RecordingPlaybackManifest,
  ShareLinkCreated,
  TaskSummary,
} from "./types";
import { authenticationRequired, redirectToLogin } from "./auth";

import { acceptBrowserCsrfToken, browserSession } from "./csrf";
import { forbiddenMessage, httpErrorMessage, sessionNotReadyMessage, unexpectedResponseMessage } from "./httpMessages";

export function initializeAppSession(token: string): void {
  browserSession.csrfToken = token;
}

export async function loadArtifact(artifactId: string): Promise<ArtifactSummary> {
  const response = await fetch(`/console/api/artifacts/${encodeURIComponent(artifactId)}`, { credentials: "same-origin", headers: { Accept: "application/json" } });
  browserSession.csrfToken = response.headers.get("x-veoveo-csrf-token") ?? browserSession.csrfToken;
  if (response.status === 401) authenticationRequired();
  if (!response.ok) throw new Error(httpErrorMessage(response.status, { action: "open this artifact", thing: "This artifact" }));
  const artifact = parseConsole("artifact", await response.json());
  if (artifact.id !== artifactId || (artifact.byteLength !== null && (!Number.isSafeInteger(artifact.byteLength) || artifact.byteLength < 0))) throw new Error(unexpectedResponseMessage);
  return artifact;
}

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
  acceptBrowserCsrfToken(response.headers.get("x-veoveo-csrf-token"));
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error("You're signed in, but your account doesn't have access to this Console. Ask an administrator for access.");
  }
  if (!response.ok) {
    throw new Error(httpErrorMessage(response.status, { action: "load the Console" }));
  }
  return parseConsole("snapshot", await response.json());
}

export async function loadCluster(signal?: AbortSignal): Promise<ClusterSnapshot> {
  const response = await fetch("/console/api/cluster", {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error(forbiddenMessage("view the cluster inventory"));
  }
  if (!response.ok) throw new Error(httpErrorMessage(response.status, { action: "load the cluster inventory" }));
  return response.json() as Promise<ClusterSnapshot>;
}

export async function consoleMutation<T>(path: string, init: RequestInit): Promise<T> {
  if (!browserSession.csrfToken) {
    throw new Error(sessionNotReadyMessage);
  }
  const headers = new Headers(init.headers);
  headers.set("Accept", "application/json");
  headers.set("Content-Type", "application/json");
  headers.set("X-Veoveo-CSRF-Token", browserSession.csrfToken);
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
    throw new Error(forbiddenMessage("make this change"));
  }
  if (!response.ok) {
    let detail: string | undefined;
    try {
      detail = ((await response.json()) as { error?: string }).error;
    } catch {
      detail = undefined;
    }
    throw new Error(detail ?? httpErrorMessage(response.status, { action: "complete this request", thing: "This item" }));
  }
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export async function logoutConsole(): Promise<void> {
  if (!browserSession.csrfToken) {
    throw new Error(sessionNotReadyMessage);
  }
  const response = await fetch("/auth/logout", {
    method: "POST",
    credentials: "same-origin",
    headers: { "X-Veoveo-CSRF-Token": browserSession.csrfToken },
    redirect: "manual"
  });
  if (!response.ok) {
    throw new Error("Veoveo couldn't sign you out. Reload the page and try again.");
  }
  browserSession.csrfToken = undefined;
  redirectToLogin();
}

export async function cancelTask(task: Pick<TaskSummary, "server" | "id">): Promise<void> {
  await consoleMutation(`tasks/${encodeURIComponent(task.server)}/${encodeURIComponent(task.id)}/cancel`, {
    method: "POST",
    body: ""
  });
}

function agentWakeReceipt(wire: AgentWakeReceiptWire): AgentWakeReceipt {
  return {
    requestId: wire.requestId,
    wakeId: wire.wakeId,
    agentId: wire.agentId,
    workContext: wire.workContext,
    acceptedAt: wire.acceptedAt,
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
      body: JSON.stringify({ requestId: requestId, message }),
    },
  );
  return agentWakeReceipt(parseAgentControl("receipt", wire));
}

export async function loadAgentConversation(
  agentId: string,
  signal?: AbortSignal,
): Promise<AgentConversation> {
  const response = await fetch(
    `/console/api/agents/${encodeURIComponent(agentId)}/conversation`,
    {
      credentials: "same-origin",
      headers: { Accept: "application/json" },
      signal,
    },
  );
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 401) authenticationRequired();
  if (response.status === 403) {
    throw new Error(forbiddenMessage("view this agent's conversation"));
  }
  if (!response.ok) throw new Error(httpErrorMessage(response.status, { action: "load the conversation", thing: "This agent" }));
  const wire = parseAgentControl("conversation", await response.json());
  return {
    agentId: wire.agentId,
    entries: wire.entries.map((entry) => ({
      entryId: entry.entryId,
      role: entry.role,
      actorId: entry.actorId,
      content: entry.content,
      state: entry.state,
      occurredAt: entry.occurredAt,
      requestId: entry.requestId,
      wakeId: entry.wakeId,
      episodeId: entry.episodeId,
      inReplyToRequestIds: entry.inReplyToRequestIds ?? [],
    })),
  };
}

export async function loadAgentInputRequests(
  agentId: string,
  signal?: AbortSignal,
): Promise<AgentInputRequest[]> {
  const response = await fetch(
    agentInputRequestsApiPath(agentId),
    {
      credentials: "same-origin",
      headers: { Accept: "application/json" },
      signal,
    },
  );
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 401) authenticationRequired();
  if (response.status === 403) {
    throw new Error(forbiddenMessage("view this agent's questions"));
  }
  if (!response.ok) throw new Error(httpErrorMessage(response.status, { action: "load the agent's questions", thing: "This agent" }));
  const values: unknown = await response.json();
  if (!Array.isArray(values)) throw new Error("Invalid agent question list");
  return values.map(value => parseAgentControl("inputRequest", value)).map((wire) => ({
    inputRequestId: wire.inputRequestId,
    message: wire.message,
    requestedSchema: wire.requestedSchema,
    requestedAt: wire.requestedAt,
  }));
}

export async function decideAgentInputRequest(
  agentId: string,
  inputRequestId: string,
  requestId: string,
  decision: { action: "accept"; content: Record<string, unknown> } | { action: "decline" | "cancel" },
): Promise<AgentWakeReceipt> {
  const wire = await consoleMutation<AgentWakeReceiptWire>(
    agentInputRequestDecisionPath(agentId, inputRequestId),
    {
      method: "POST",
      body: JSON.stringify({ requestId: requestId, ...decision }),
    },
  );
  return agentWakeReceipt(parseAgentControl("receipt", wire));
}

export async function setArtifactReleaseState(artifactId: string, releaseState: ReleaseState): Promise<void> {
  await consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/release-state`, {
    method: "PUT",
    body: JSON.stringify({ releaseState })
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

function artifactAccessRequest(wire: ArtifactAccessRequestWire): ArtifactAccessRequest {
  return {
    id: wire.id,
    artifactId: wire.artifactId,
    workContext: wire.workContext,
    requester: wire.requester,
    requestedLevel: wire.requestedLevel,
    justification: wire.justification,
    state: wire.state,
    decidedBy: wire.decidedBy,
    decisionNote: wire.decisionNote,
    createdAt: wire.createdAt,
    updatedAt: wire.updatedAt,
    decidedAt: wire.decidedAt,
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
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 401) {
    authenticationRequired();
  }
  if (response.status === 403) {
    throw new Error(forbiddenMessage("view access requests in this Work Context"));
  }
  if (!response.ok) throw new Error(httpErrorMessage(response.status, { action: "load access requests" }));
  const page = (await response.json()) as ArtifactAccessRequestPageWire;
  return {
    requests: page.requests.map(artifactAccessRequest),
    nextCursor: page.nextCursor,
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
        requestedLevel,
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
      expiresAt,
      ...(maxDownloads ? { maxDownloads } : {})
    })
  });
}

export async function revokeArtifactShareLink(artifactId: string, linkId: string): Promise<void> {
  await consoleMutation(
    `artifacts/${encodeURIComponent(artifactId)}/share-links/${encodeURIComponent(linkId)}`,
    { method: "DELETE", body: "" }
  );
}

export async function loadRecordingPlayback(
  recordingId: string,
  options: { signal?: AbortSignal; recordingGrant?: string } = {}
): Promise<RecordingPlaybackManifest> {
  const headers: Record<string, string> = { Accept: "application/json" };
  if (options.recordingGrant) {
    headers["X-Veoveo-Recording-Grant"] = options.recordingGrant;
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
    throw new Error(forbiddenMessage("play this recording"));
  }
  if (!response.ok) {
    throw new Error(httpErrorMessage(response.status, { action: "start playback", thing: "This recording" }));
  }
  return recordingPlaybackValue(await response.json(), recordingId);
}

export function recordingLiveRrdStreamRoute(recordingId: string): string {
  const path = `/console/api/recordings/${encodeURIComponent(recordingId)}/live/rrd-stream`;
  return new URL(path, window.location.origin).toString();
}

export function recordingBlueprintUrl(recordingId: string, revision: number): string {
  const path = `/console/api/recordings/${encodeURIComponent(recordingId)}/blueprints/${encodeURIComponent(revision)}/data.rrd`;
  return new URL(path, window.location.origin).toString();
}

export interface RecordingProjectionStream {
  stream: ReadableStream<Uint8Array>;
  byteLength: number;
  sha256: string;
}

export async function loadRecordingProjectionStream(
  recordingId: string,
  projectionId: string,
  signal?: AbortSignal,
): Promise<RecordingProjectionStream> {
  const path = `/console/api/recordings/${encodeURIComponent(recordingId)}/projections/${encodeURIComponent(projectionId)}/data.arrow`;
  const response = await fetch(path, {
    credentials: "same-origin",
    headers: { Accept: "application/vnd.apache.arrow.stream" },
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 401) authenticationRequired();
  if (response.status === 403) {
    throw new Error(forbiddenMessage("read this recording data"));
  }
  if (!response.ok) throw new Error(httpErrorMessage(response.status, { action: "load the recording data", thing: "This recording data" }));
  const byteLength = Number(response.headers.get("content-length"));
  const sha256 = response.headers.get("x-veoveo-payload-sha256") ?? "";
  if (
    response.headers.get("content-type") !== "application/vnd.apache.arrow.stream" ||
    !Number.isSafeInteger(byteLength) ||
    byteLength < 0 ||
    byteLength > 32 * 1024 * 1024 ||
    !/^[0-9a-f]{64}$/i.test(sha256) ||
    response.body === null
  ) {
    response.body?.cancel().catch(() => undefined);
    throw new Error("The recording data couldn't be read. Refresh the page and try again.");
  }
  return { stream: response.body, byteLength, sha256 };
}

export async function loadApps(signal?: AbortSignal): Promise<AppCatalog> {
  const response = await fetch("/console/api/apps", {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 401) {
    authenticationRequired();
  }
  if (!response.ok) throw new Error(httpErrorMessage(response.status, { action: "load the app catalog" }));
  return response.json() as Promise<AppCatalog>;
}

export function appFrameUrl(resourceUri: string): string {
  return `/console/api/apps/frame?uri=${encodeURIComponent(resourceUri)}`;
}

export async function callAppTool(
  server: string,
  appUri: string,
  tool: string,
  toolArguments: Record<string, unknown>,
  extras: AppToolRequestExtras = {},
): Promise<AppToolResult> {
  return consoleMutation<AppToolResult>("apps/call", {
    method: "POST",
    body: JSON.stringify({ server, appUri, tool, arguments: toolArguments, ...extras }),
  });
}

export async function getAppTask(
  server: string,
  appUri: string,
  taskId: string
): Promise<TaskDetailResult> {
  return consoleMutation<TaskDetailResult>("apps/task/get", {
    method: "POST",
    body: JSON.stringify({ server, appUri, taskId }),
  });
}

export async function updateAppTask(
  server: string,
  appUri: string,
  taskId: string,
  inputResponses: InputResponses,
): Promise<TaskAckResult> {
  return consoleMutation<TaskAckResult>("apps/task/update", {
    method: "POST",
    body: JSON.stringify({ server, appUri, taskId, inputResponses }),
  });
}

export async function cancelAppTask(
  server: string,
  appUri: string,
  taskId: string
): Promise<TaskAckResult> {
  return consoleMutation<TaskAckResult>("apps/task/cancel", {
    method: "POST",
    body: JSON.stringify({ server, appUri, taskId }),
  });
}

export async function readAppResource(
  server: string,
  appUri: string,
  uri: string
): Promise<AppReadResourceResult> {
  return consoleMutation<AppReadResourceResult>("apps/read", {
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
  if (!browserSession.csrfToken) throw new Error(sessionNotReadyMessage);
  const response = await fetch("/console/api/apps/resource-events", {
    method: "POST",
    credentials: "same-origin",
    headers: {
      Accept: "text/event-stream",
      "Content-Type": "application/json",
      "X-Veoveo-CSRF-Token": browserSession.csrfToken,
    },
    body: JSON.stringify({ server, appUri, subscriptions }),
    signal,
  });
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) browserSession.csrfToken = rotatedToken;
  if (response.status === 401) authenticationRequired();
  if (response.status === 403) {
    throw new Error(forbiddenMessage("follow live updates in this App"));
  }
  return response;
}

export async function unsubscribeAppResource(subscriptionId: string): Promise<void> {
  await consoleMutation<Record<string, never>>("apps/resource-unsubscribe", {
    method: "POST",
    body: JSON.stringify({ subscriptionId }),
  });
}
