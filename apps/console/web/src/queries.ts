import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import {
  cancelArtifactAccessRequest,
  cancelTask,
  createArtifactShareLink,
  decideArtifactAccessRequest,
  grantArtifact,
  loadApps,
  loadCluster,
  loadArtifactAccessRequests,
  loadSnapshot,
  revokeArtifactGrant,
  revokeArtifactShareLink,
  requestArtifactAccess,
  setArtifactReleaseState,
} from "./api";
import type {
  ArtifactAccessRequestState,
  ArtifactSummary,
  InstallationSnapshot,
  ReleaseState,
  ShareLinkCreated,
} from "./types";

export const queryKeys = {
  snapshot: ["snapshot"] as const,
  apps: ["apps"] as const,
  cluster: ["cluster"] as const,
  accessRequests: ["artifact-access-requests"] as const,
};

export function useSnapshot() {
  return useQuery({
    queryKey: queryKeys.snapshot,
    queryFn: ({ signal }) => loadSnapshot(signal),
    // The live stream keeps the snapshot current; background refetch would
    // only race it. Stream resets invalidate explicitly.
    staleTime: Infinity,
  });
}

export function useCluster() {
  return useQuery({
    queryKey: queryKeys.cluster,
    queryFn: ({ signal }) => loadCluster(signal),
  });
}

// The MCP app catalog is discovery data loaded once for the browser session.
// A new deployment starts a new catalog generation and console session.
export function useApps() {
  return useQuery({
    queryKey: queryKeys.apps,
    queryFn: ({ signal }) => loadApps(signal),
    staleTime: Infinity,
  });
}

export function useArtifactAccessRequests(
  scope: "mine" | "reviewable",
  state?: ArtifactAccessRequestState,
  enabled = true
) {
  return useQuery({
    queryKey: [...queryKeys.accessRequests, scope, state ?? "all"],
    queryFn: ({ signal }) => loadArtifactAccessRequests(scope, state, signal),
    staleTime: Infinity,
    enabled,
  });
}

function refreshAccessRequests(client: QueryClient) {
  return client.invalidateQueries({ queryKey: queryKeys.accessRequests });
}

export function useRequestArtifactAccess() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      artifactId,
      requestedLevel,
      justification,
    }: {
      artifactId: string;
      requestedLevel: "read" | "write" | "admin";
      justification: string;
    }) => requestArtifactAccess(artifactId, requestedLevel, justification),
    onSuccess: () => refreshAccessRequests(client),
  });
}

export function useDecideArtifactAccessRequest() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      requestId,
      decision,
      note,
    }: {
      requestId: string;
      decision: "approve" | "deny";
      note?: string;
    }) => decideArtifactAccessRequest(requestId, decision, note),
    onSuccess: () => refreshAccessRequests(client),
  });
}

export function useCancelArtifactAccessRequest() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (requestId: string) => cancelArtifactAccessRequest(requestId),
    onSuccess: () => refreshAccessRequests(client),
  });
}

function patchSnapshot(client: QueryClient, patch: (snapshot: InstallationSnapshot) => InstallationSnapshot) {
  client.setQueryData<InstallationSnapshot>(queryKeys.snapshot, (current) => (current ? patch(current) : current));
}

function patchArtifact(
  client: QueryClient,
  artifactId: string,
  patch: (artifact: ArtifactSummary) => ArtifactSummary
) {
  patchSnapshot(client, (snapshot) => ({
    ...snapshot,
    artifacts: snapshot.artifacts.map((artifact) => (artifact.id === artifactId ? patch(artifact) : artifact)),
  }));
}

export function useCancelTask() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (taskId: string) => cancelTask(taskId),
    onSuccess: (_result, taskId) => {
      patchSnapshot(client, (snapshot) => ({
        ...snapshot,
        tasks: snapshot.tasks.map((task) =>
          task.id === taskId ? { ...task, state: "cancel_requested" as const } : task
        ),
      }));
    },
  });
}

export function useSetArtifactReleaseState() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ artifactId, releaseState }: { artifactId: string; releaseState: ReleaseState }) =>
      setArtifactReleaseState(artifactId, releaseState),
    onSuccess: (_result, { artifactId, releaseState }) => {
      patchArtifact(client, artifactId, (artifact) => ({ ...artifact, releaseState }));
    },
  });
}

export function useGrantArtifact() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      artifactId,
      subject,
      level,
    }: {
      artifactId: string;
      subject: { kind: "principal" | "group"; id: string };
      level: "read" | "write" | "admin";
    }) => grantArtifact(artifactId, subject, level),
    onSuccess: (_result, { artifactId, subject, level }) => {
      patchArtifact(client, artifactId, (artifact) => {
        const grants = artifact.grants.filter(
          (grant) => !(grant.subjectKind === subject.kind && grant.subject === subject.id)
        );
        grants.push({
          subjectKind: subject.kind,
          subject: subject.id,
          permission: level,
          labels: [],
          createdAt: new Date().toISOString(),
        });
        return { ...artifact, grants, authorizedGrants: grants.length };
      });
    },
  });
}

export function useRevokeArtifactGrant() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ artifactId, subject }: { artifactId: string; subject: { kind: "principal" | "group"; id: string } }) =>
      revokeArtifactGrant(artifactId, subject),
    onSuccess: (_result, { artifactId, subject }) => {
      patchArtifact(client, artifactId, (artifact) => {
        const grants = artifact.grants.filter(
          (grant) => !(grant.subjectKind === subject.kind && grant.subject === subject.id)
        );
        return { ...artifact, grants, authorizedGrants: grants.length };
      });
    },
  });
}

export function useCreateShareLink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      artifactId,
      expiresAt,
      maxDownloads,
    }: {
      artifactId: string;
      expiresAt: string;
      maxDownloads?: number;
    }): Promise<ShareLinkCreated> => createArtifactShareLink(artifactId, expiresAt, maxDownloads),
    onSuccess: (created, { artifactId }) => {
      patchArtifact(client, artifactId, (artifact) => ({
        ...artifact,
        activeLinks: artifact.activeLinks + 1,
        shareLinks: [
          {
            id: created.link_id,
            permission: "read" as const,
            expiresAt: created.expires_at,
            maxDownloads: created.max_downloads,
            downloadCount: 0,
            createdAt: new Date().toISOString(),
            active: true,
          },
          ...artifact.shareLinks,
        ],
      }));
    },
  });
}

export function useRevokeShareLink() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ artifactId, linkId }: { artifactId: string; linkId: string }) =>
      revokeArtifactShareLink(artifactId, linkId),
    onSuccess: (_result, { artifactId, linkId }) => {
      patchArtifact(client, artifactId, (artifact) => ({
        ...artifact,
        activeLinks: Math.max(0, artifact.activeLinks - 1),
        shareLinks: artifact.shareLinks.map((link) =>
          link.id === linkId ? { ...link, active: false, revokedAt: new Date().toISOString() } : link
        ),
      }));
    },
  });
}
