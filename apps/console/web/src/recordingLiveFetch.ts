const UUID_V7 =
  "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const LIVE_PLAYBACK_PATH = new RegExp(
  `^/console/api/recordings/${UUID_V7}/segments/${UUID_V7}/live\\.rrd$`
);

interface FetchAdapterInstallation {
  consumers: number;
  original: typeof globalThis.fetch;
  adapted: typeof globalThis.fetch;
}

let installation: FetchAdapterInstallation | undefined;

export function isConsoleLivePlaybackRequest(
  request: Request,
  consoleOrigin: string
): boolean {
  if (request.method !== "GET") return false;
  const url = new URL(request.url);
  return (
    url.origin === consoleOrigin &&
    url.search === "" &&
    url.hash === "" &&
    LIVE_PLAYBACK_PATH.test(url.pathname)
  );
}

export function attachConsoleSessionToLivePlayback(
  request: Request,
  consoleOrigin: string
): Request {
  if (
    request.credentials !== "omit" ||
    !isConsoleLivePlaybackRequest(request, consoleOrigin)
  ) {
    return request;
  }
  return new Request(request, { credentials: "same-origin" });
}

/**
 * Rerun 0.35 deliberately omits credentials on HTTP RRD receivers. Veoveo's
 * bounded live receiver is a same-origin Console resource, so its existing
 * HttpOnly session cookie must be restored at the browser Fetch boundary.
 *
 * The adapter is reversible and exact-path only. It cannot add credentials to
 * Redap, arbitrary HTTP sources, legacy archive routes, or cross-origin URLs.
 */
export function installConsoleLivePlaybackFetch(): () => void {
  if (installation) {
    installation.consumers += 1;
    return releaseOnce();
  }

  const original = globalThis.fetch;
  const origin = globalThis.location.origin;
  const adapted: typeof globalThis.fetch = (input, init) => {
    let request: Request;
    try {
      request = new Request(input, init);
    } catch {
      return original.call(globalThis, input, init);
    }
    const authorized = attachConsoleSessionToLivePlayback(request, origin);
    return authorized === request
      ? original.call(globalThis, input, init)
      : original.call(globalThis, authorized);
  };
  installation = {
    consumers: 1,
    original,
    adapted,
  };
  globalThis.fetch = adapted;
  return releaseOnce();
}

function releaseOnce(): () => void {
  let released = false;
  return () => {
    if (released) return;
    released = true;
    const current = installation;
    if (!current) return;
    current.consumers -= 1;
    if (current.consumers > 0) return;
    if (globalThis.fetch === current.adapted) {
      globalThis.fetch = current.original;
    }
    installation = undefined;
  };
}
