import { demoSnapshot } from "./demo";
import type { CallToolResult, ReadResourceResult } from "@modelcontextprotocol/sdk/types.js";
import type {
  AppCatalog,
  InstallationSnapshot,
  ClusterSnapshot,
  ReleaseState,
  RecordingPlaybackManifest,
  ShareLinkCreated,
} from "./types";

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
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
  }
  if (response.status === 403) {
    throw new Error(
      "Your Microsoft Entra account is signed in but is not assigned the veoveo_admin application role."
    );
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
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
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
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
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
  window.location.assign("/auth/login");
}

export async function cancelTask(taskId: string): Promise<void> {
  await consoleMutation(`tasks/${encodeURIComponent(taskId)}/cancel`, {
    method: "POST",
    body: ""
  });
}

export async function setArtifactReleaseState(artifactId: string, releaseState: ReleaseState): Promise<void> {
  await consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/release-state`, {
    method: "PUT",
    body: JSON.stringify({ release_state: releaseState })
  });
}

export async function grantArtifact(
  artifactId: string,
  subject: { kind: "user" | "group"; id: string },
  level: "read" | "write" | "admin"
): Promise<void> {
  await consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/grants`, {
    method: "POST",
    body: JSON.stringify({ subject, level })
  });
}

export async function revokeArtifactGrant(
  artifactId: string,
  subject: { kind: "user" | "group"; id: string }
): Promise<void> {
  await consoleMutation(`artifacts/${encodeURIComponent(artifactId)}/grants`, {
    method: "DELETE",
    body: JSON.stringify(subject)
  });
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
  signal?: AbortSignal
): Promise<RecordingPlaybackManifest> {
  const response = await fetch(
    `/console/api/recordings/${encodeURIComponent(recordingId)}/playback`,
    {
      credentials: "same-origin",
      headers: { Accept: "application/json" },
      signal,
    }
  );
  if (response.status === 401) {
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
  }
  if (response.status === 403) {
    throw new Error("Playback is not permitted by the active recording policy.");
  }
  if (!response.ok) {
    throw new Error(`Recording playback returned ${response.status}`);
  }
  return response.json() as Promise<RecordingPlaybackManifest>;
}

export function recordingSegmentUrl(
  recordingId: string,
  ticket: string,
  segmentId: string
): string {
  const path = `/console/api/recordings/${encodeURIComponent(recordingId)}/sources/${encodeURIComponent(ticket)}/segments/${encodeURIComponent(segmentId)}/data.rrd`;
  return new URL(path, window.location.origin).toString();
}

export function recordingLiveSegmentUrl(
  recordingId: string,
  ticket: string,
  segmentId: string
): string {
  const path = `/console/api/recordings/${encodeURIComponent(recordingId)}/sources/${encodeURIComponent(ticket)}/segments/${encodeURIComponent(segmentId)}/live.rrd`;
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
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
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
): Promise<CallToolResult> {
  return consoleMutation<CallToolResult>("apps/call", {
    method: "POST",
    body: JSON.stringify({ server, appUri, tool, arguments: toolArguments }),
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
