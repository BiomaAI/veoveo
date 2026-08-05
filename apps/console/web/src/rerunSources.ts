export interface GovernedRerunArchive {
  uri: string;
  revision: string;
}

export type GovernedRerunReceiver =
  | { kind: "live"; route: string; viewerUri: string; generation: number }
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
  liveRoute: string | undefined,
  liveViewerUri: string | undefined,
  liveGeneration = 0
): SelectedRerunPlaybackReceiver {
  if (requestedMode === "live" && liveRoute && liveViewerUri) {
    return {
      mode: "live",
      receiver: {
        kind: "live",
        route: liveRoute,
        viewerUri: liveViewerUri,
        generation: liveGeneration,
      },
    };
  }
  if (archive) {
    return { mode: "archive", receiver: { kind: "archive", archive } };
  }
  if (liveRoute && liveViewerUri) {
    return {
      mode: "live",
      receiver: {
        kind: "live",
        route: liveRoute,
        viewerUri: liveViewerUri,
        generation: liveGeneration,
      },
    };
  }
  return { mode: requestedMode };
}

export interface RerunSourceTransition {
  credentialsChanged: boolean;
  urlsToCloseBeforeOpen: string[];
  blueprintUrlToOpen?: string;
  receiverUrlToOpen?: string;
  next: OpenedRerunSources;
}

function receiverUrl(receiver: GovernedRerunReceiver | undefined) {
  if (!receiver) return undefined;
  return receiver.kind === "live" ? receiver.viewerUri : receiver.archive.uri;
}

function receiversEqual(
  opened: GovernedRerunReceiver | undefined,
  desired: GovernedRerunReceiver
) {
  if (!opened || opened.kind !== desired.kind) return false;
  if (opened.kind === "live" && desired.kind === "live") {
    return (
      opened.route === desired.route &&
      opened.viewerUri === desired.viewerUri &&
      opened.generation === desired.generation
    );
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
  if (receiverChanged) {
    const openedReceiverUrl = receiverUrl(opened.receiver);
    if (openedReceiverUrl) urlsToCloseBeforeOpen.push(openedReceiverUrl);
  }
  if (blueprintChanged && opened.blueprintUrl) {
    urlsToCloseBeforeOpen.push(opened.blueprintUrl);
  }
  return {
    credentialsChanged: opened.redapToken !== desired.redapToken,
    urlsToCloseBeforeOpen,
    blueprintUrlToOpen: blueprintChanged ? desired.blueprintUrl : undefined,
    receiverUrlToOpen: receiverChanged
      ? receiverUrl(desired.receiver)
      : undefined,
    next: {
      redapToken: desired.redapToken,
      receiver: desired.receiver,
      blueprintUrl: desired.blueprintUrl,
      blueprintMapProvider: desired.blueprintMapProvider,
    },
  };
}
