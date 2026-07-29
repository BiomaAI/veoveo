export interface GovernedRerunArchive {
  uri: string;
  revision: string;
}

export interface GovernedRerunSource {
  redapToken: string;
  archive?: GovernedRerunArchive;
  liveUrl?: string;
}

export interface OpenedRerunSources {
  redapToken?: string;
  archive?: GovernedRerunArchive;
  liveUrl?: string;
}

export interface RerunSourceTransition {
  credentialsChanged: boolean;
  archiveUrlToCloseBeforeOpen?: string;
  archiveUrlToOpen?: string;
  liveUrlToOpen?: string;
  urlsToCloseAfterOpen: string[];
  next: OpenedRerunSources;
}

export function planRerunSourceTransition(
  opened: OpenedRerunSources,
  desired: GovernedRerunSource
): RerunSourceTransition {
  const archiveChanged =
    opened.archive?.uri !== desired.archive?.uri ||
    opened.archive?.revision !== desired.archive?.revision;
  const sameArchiveReceiverNeedsRefresh =
    archiveChanged &&
    opened.archive?.uri !== undefined &&
    opened.archive.uri === desired.archive?.uri;
  const urlsToCloseAfterOpen: string[] = [];
  if (
    archiveChanged &&
    opened.archive &&
    opened.archive.uri !== desired.archive?.uri
  ) {
    urlsToCloseAfterOpen.push(opened.archive.uri);
  }
  if (opened.liveUrl && opened.liveUrl !== desired.liveUrl) {
    urlsToCloseAfterOpen.push(opened.liveUrl);
  }
  return {
    credentialsChanged: opened.redapToken !== desired.redapToken,
    archiveUrlToCloseBeforeOpen: sameArchiveReceiverNeedsRefresh
      ? opened.archive?.uri
      : undefined,
    archiveUrlToOpen: archiveChanged ? desired.archive?.uri : undefined,
    liveUrlToOpen:
      desired.liveUrl && desired.liveUrl !== opened.liveUrl
        ? desired.liveUrl
        : undefined,
    urlsToCloseAfterOpen,
    next: {
      redapToken: desired.redapToken,
      archive: desired.archive,
      liveUrl: desired.liveUrl,
    },
  };
}
