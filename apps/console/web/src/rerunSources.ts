export interface GovernedRerunArchive {
  uri: string;
  revision: string;
}

export type GovernedRerunReceiver =
  | { kind: "live"; route: string }
  | { kind: "archive"; archive: GovernedRerunArchive };

export interface GovernedRerunSource {
  redapToken: string;
  receiver: GovernedRerunReceiver;
  blueprintUrl?: string;
  blueprintMapProvider?: "none" | "openStreetMap" | "mapbox" | "mixed";
}

export interface OpenedRerunSources {
  redapToken?: string;
  receiver?: GovernedRerunReceiver;
  blueprintUrl?: string;
  blueprintMapProvider?: "none" | "openStreetMap" | "mapbox" | "mixed";
}

export type RerunPlaybackMode = "live" | "archive";

export function requiresPlaybackCredentialRenewal(
  receiver: GovernedRerunReceiver | undefined
): boolean {
  return receiver?.kind === "archive";
}

export interface SelectedRerunPlaybackReceiver {
  mode: RerunPlaybackMode;
  receiver?: GovernedRerunReceiver;
}

export function selectExclusiveRerunPlaybackReceiver(
  requestedMode: RerunPlaybackMode,
  archive: GovernedRerunArchive | undefined,
  liveRoute: string | undefined
): SelectedRerunPlaybackReceiver {
  if (requestedMode === "live" && liveRoute) {
    return { mode: "live", receiver: { kind: "live", route: liveRoute } };
  }
  if (archive) {
    return { mode: "archive", receiver: { kind: "archive", archive } };
  }
  if (liveRoute) {
    return { mode: "live", receiver: { kind: "live", route: liveRoute } };
  }
  return { mode: requestedMode };
}

export interface RerunSourceTransition {
  credentialsChanged: boolean;
  urlsToCloseBeforeOpen: string[];
  closeLiveConnection: boolean;
  blueprintUrlToOpen?: string;
  archiveUrlToOpen?: string;
  liveRouteToOpen?: string;
  next: OpenedRerunSources;
}

function receiversEqual(
  opened: GovernedRerunReceiver | undefined,
  desired: GovernedRerunReceiver
) {
  if (!opened || opened.kind !== desired.kind) return false;
  if (opened.kind === "live" && desired.kind === "live") {
    return opened.route === desired.route;
  }
  if (opened.kind === "archive" && desired.kind === "archive") {
    return (
      opened.archive.uri === desired.archive.uri &&
      opened.archive.revision === desired.archive.revision
    );
  }
  return false;
}

export function planRerunSourceTransition(
  opened: OpenedRerunSources,
  desired: GovernedRerunSource
): RerunSourceTransition {
  const receiverChanged = !receiversEqual(opened.receiver, desired.receiver);
  const blueprintChanged = opened.blueprintUrl !== desired.blueprintUrl;
  const urlsToCloseBeforeOpen: string[] = [];
  if (receiverChanged && opened.receiver?.kind === "archive") {
    urlsToCloseBeforeOpen.push(opened.receiver.archive.uri);
  }
  if (blueprintChanged && opened.blueprintUrl) {
    urlsToCloseBeforeOpen.push(opened.blueprintUrl);
  }
  return {
    credentialsChanged: opened.redapToken !== desired.redapToken,
    urlsToCloseBeforeOpen,
    closeLiveConnection: receiverChanged && opened.receiver?.kind === "live",
    blueprintUrlToOpen: blueprintChanged ? desired.blueprintUrl : undefined,
    archiveUrlToOpen:
      receiverChanged && desired.receiver.kind === "archive"
        ? desired.receiver.archive.uri
        : undefined,
    liveRouteToOpen:
      receiverChanged && desired.receiver.kind === "live"
        ? desired.receiver.route
        : undefined,
    next: {
      redapToken: desired.redapToken,
      receiver: desired.receiver,
      blueprintUrl: desired.blueprintUrl,
      blueprintMapProvider: desired.blueprintMapProvider,
    },
  };
}
