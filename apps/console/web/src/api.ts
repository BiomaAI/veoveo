import { demoSnapshot } from "./demo";
import type {
  InstallationSnapshot,
  MapActiveReleaseSummary,
  MapAcquisitionSummary,
  MapMobilityProfileSummary,
  MapReleaseSummary,
  MapSourceSummary,
  ReleaseState,
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

export async function mapAdminQuery<T>(path: string): Promise<T> {
  const response = await fetch(`/console/api/map/${path.replace(/^\/+/, "")}`, {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
  });
  if (response.status === 401) {
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
  }
  if (!response.ok) throw new Error(`Map administration returned ${response.status}`);
  const rotatedToken = response.headers.get("x-veoveo-csrf-token");
  if (rotatedToken) csrfToken = rotatedToken;
  return response.json() as Promise<T>;
}

interface MapAdminPage<T> { items: T[]; next_cursor?: string }
export const loadMapSources = async () => (await mapAdminQuery<MapAdminPage<MapSourceSummary>>("sources?limit=200")).items;
export const loadMapAcquisitions = async () => (await mapAdminQuery<MapAdminPage<MapAcquisitionSummary>>("acquisitions?limit=200")).items;
export const loadMapReleases = async () => (await mapAdminQuery<MapAdminPage<MapReleaseSummary>>("releases?limit=200")).items;
export const loadMapMobilityProfiles = async () => (await mapAdminQuery<MapAdminPage<MapMobilityProfileSummary>>("mobility-profiles?limit=200")).items;
export const loadMapActiveReleases = async () => (await mapAdminQuery<MapAdminPage<MapActiveReleaseSummary>>("active-releases?limit=200")).items;

export const loadMapAdministration = async () => {
  const sources = await loadMapSources();
  const acquisitions = await loadMapAcquisitions();
  const releases = await loadMapReleases();
  const mobilityProfiles = await loadMapMobilityProfiles();
  const activeReleases = await loadMapActiveReleases();
  return { sources, acquisitions, releases, mobilityProfiles, activeReleases };
};

export async function registerMapSource(source: unknown): Promise<MapSourceSummary> {
  return consoleMutation("map/sources", {
    method: "POST",
    body: JSON.stringify({ source, idempotency_key: crypto.randomUUID() }),
  });
}

export async function registerMapMobilityProfile(profile: unknown): Promise<MapMobilityProfileSummary> {
  return consoleMutation("map/mobility-profiles", {
    method: "POST",
    body: JSON.stringify({ profile, idempotency_key: crypto.randomUUID() }),
  });
}

export async function startMapAcquisition(
  sourceId: string,
  coverage: { west: number; south: number; east: number; north: number },
): Promise<MapAcquisitionSummary> {
  return consoleMutation("map/acquisitions", {
    method: "POST",
    body: JSON.stringify({
      source_id: sourceId,
      requested_coverage: coverage,
      idempotency_key: crypto.randomUUID(),
    }),
  });
}

export async function mutateMapRelease(
  release: MapReleaseSummary,
  action: "activate" | "rollback" | "quarantine",
  activePointerVersion: number,
): Promise<unknown> {
  return consoleMutation(`map/releases/${encodeURIComponent(release.release_id)}/${action}`, {
    method: "POST",
    body: JSON.stringify({
      expected_record_version: release.record_version,
      expected_active_pointer_version: activePointerVersion,
    }),
  });
}
