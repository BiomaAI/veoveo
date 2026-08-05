export interface GovernedRerunArchive {
  uri: string;
  revision: string;
}

export type GovernedRerunReceiver =
  | { kind: "live"; url: string }
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
  liveUrl: string | undefined
): SelectedRerunPlaybackReceiver {
  if (requestedMode === "live" && liveUrl) {
    return { mode: "live", receiver: { kind: "live", url: liveUrl } };
  }
  if (archive) {
    return { mode: "archive", receiver: { kind: "archive", archive } };
  }
  if (liveUrl) {
    return { mode: "live", receiver: { kind: "live", url: liveUrl } };
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

export function newestLiveTime(
  currentTime: number,
  range: { min: number; max: number } | null,
  live: boolean
): number | undefined {
  if (!live || !range || !Number.isFinite(currentTime) || !Number.isFinite(range.max)) {
    return undefined;
  }
  return range.max > currentTime ? range.max : undefined;
}

function receiverUrl(receiver: GovernedRerunReceiver | undefined) {
  return receiver?.kind === "archive" ? receiver.archive.uri : receiver?.url;
}

function receiversEqual(
  opened: GovernedRerunReceiver | undefined,
  desired: GovernedRerunReceiver
) {
  if (!opened || opened.kind !== desired.kind) return false;
  if (opened.kind === "live" && desired.kind === "live") {
    return opened.url === desired.url;
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
