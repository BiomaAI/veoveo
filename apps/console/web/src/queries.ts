import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import {
  cancelTask,
  createArtifactShareLink,
  grantArtifact,
  loadCluster,
  loadMapAdministration,
  loadSnapshot,
  mutateMapRelease,
  registerMapMobilityProfile,
  registerMapSource,
  revokeArtifactGrant,
  revokeArtifactShareLink,
  setArtifactReleaseState,
  startMapAcquisition,
} from "./api";
import type {
  ArtifactSummary,
  InstallationSnapshot,
  MapReleaseSummary,
  ReleaseState,
  ShareLinkCreated,
} from "./types";

export const queryKeys = {
  snapshot: ["snapshot"] as const,
  mapAdmin: ["mapAdmin"] as const,
  cluster: ["cluster"] as const,
};

export function useSnapshot() {
  return useQuery({
    queryKey: queryKeys.snapshot,
    queryFn: ({ signal }) => loadSnapshot(signal),
    // The live stream keeps the snapshot current; background refetch would
    // only race it. Manual refresh and stream resets invalidate explicitly.
    staleTime: Infinity,
  });
}

export function useCluster() {
  return useQuery({
    queryKey: queryKeys.cluster,
    queryFn: ({ signal }) => loadCluster(signal),
  });
}

export function useMapAdministration() {
  return useQuery({
    queryKey: queryKeys.mapAdmin,
    queryFn: ({ signal }) => loadMapAdministration(signal),
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
      subject: { kind: "user" | "group"; id: string };
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
    mutationFn: ({ artifactId, subject }: { artifactId: string; subject: { kind: "user" | "group"; id: string } }) =>
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

function useMapMutation<Args>(mutationFn: (args: Args) => Promise<unknown>) {
  const client = useQueryClient();
  return useMutation({
    mutationFn,
    onSuccess: () => client.invalidateQueries({ queryKey: queryKeys.mapAdmin }),
  });
}

export function useRegisterMapSource() {
  return useMapMutation((source: unknown) => registerMapSource(source));
}

export function useRegisterMapMobilityProfile() {
  return useMapMutation((profile: unknown) => registerMapMobilityProfile(profile));
}

export function useStartMapAcquisition() {
  return useMapMutation(
    ({
      sourceId,
      coverage,
    }: {
      sourceId: string;
      coverage: { west: number; south: number; east: number; north: number };
    }) => startMapAcquisition(sourceId, coverage)
  );
}

export function useMutateMapRelease() {
  return useMapMutation(
    ({
      release,
      action,
      activePointerVersion,
    }: {
      release: MapReleaseSummary;
      action: "activate" | "rollback" | "quarantine";
      activePointerVersion: number;
    }) => mutateMapRelease(release, action, activePointerVersion)
  );
}
