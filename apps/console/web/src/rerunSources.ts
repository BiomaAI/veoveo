export interface GovernedRerunSource {
  archiveUrls: string[];
  liveUrl?: string;
}

export interface OpenedRerunSources {
  archiveUrls: Set<string>;
  liveUrl?: string;
}

export interface RerunSourceTransition {
  archiveUrlsToOpen: string[];
  liveUrlToOpen?: string;
  urlsToClose: string[];
  next: OpenedRerunSources;
}

export function planRerunSourceTransition(
  opened: OpenedRerunSources,
  desired: GovernedRerunSource
): RerunSourceTransition {
  const desiredArchiveUrls = new Set(desired.archiveUrls);
  const archiveUrlsToOpen = desired.archiveUrls.filter(
    (url) => !opened.archiveUrls.has(url)
  );
  const urlsToClose = [...opened.archiveUrls].filter(
    (url) => !desiredArchiveUrls.has(url)
  );
  if (opened.liveUrl && opened.liveUrl !== desired.liveUrl) {
    urlsToClose.push(opened.liveUrl);
  }
  return {
    archiveUrlsToOpen,
    liveUrlToOpen:
      desired.liveUrl && desired.liveUrl !== opened.liveUrl
        ? desired.liveUrl
        : undefined,
    urlsToClose,
    next: {
      archiveUrls: desiredArchiveUrls,
      liveUrl: desired.liveUrl,
    },
  };
}
