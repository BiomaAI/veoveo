import { demoSnapshot } from "./demo";
import type { InstallationSnapshot, ReleaseState, ShareLinkCreated } from "./types";

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
  if (response.status === 401) {
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
  }
  if (!response.ok) {
    throw new Error(`Console API returned ${response.status}`);
  }
  csrfToken = response.headers.get("x-veoveo-csrf-token") ?? undefined;
  return response.json() as Promise<InstallationSnapshot>;
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
  if (!response.ok) {
    throw new Error(`Console API returned ${response.status}`);
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
