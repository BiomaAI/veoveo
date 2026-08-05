const UUID_V7 =
  "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const GOVERNED_RRD_PATH = new RegExp(
  `^/console/api/recordings/${UUID_V7}/(?:segments/${UUID_V7}/live\\.rrd|blueprints/[1-9][0-9]*/data\\.rrd)$`
);
const LIVE_RRD_PATH = new RegExp(
  `^/console/api/recordings/${UUID_V7}/segments/${UUID_V7}/live\\.rrd$`
);

interface FetchAdapterInstallation {
  consumers: number;
  liveEndObservers: Set<(url: string) => void>;
  original: typeof globalThis.fetch;
  adapted: typeof globalThis.fetch;
}

let installation: FetchAdapterInstallation | undefined;

export function isConsoleRecordingRrdRequest(
  request: Request,
  consoleOrigin: string
): boolean {
  if (request.method !== "GET") return false;
  const url = new URL(request.url);
  return (
    url.origin === consoleOrigin &&
    url.search === "" &&
    url.hash === "" &&
    GOVERNED_RRD_PATH.test(url.pathname)
  );
}

export function attachConsoleSessionToRecordingRrd(
  request: Request,
  consoleOrigin: string
): Request {
  if (
    request.credentials !== "omit" ||
    !isConsoleRecordingRrdRequest(request, consoleOrigin)
  ) {
    return request;
  }
  return new Request(request, { credentials: "same-origin" });
}

export function authorizeConsoleRecordingRrdFetch(
  input: RequestInfo | URL,
  init: RequestInit | undefined,
  consoleOrigin: string
): readonly [RequestInfo | URL, RequestInit | undefined] {
  const request = input instanceof Request ? input : undefined;
  const method = (init?.method ?? request?.method ?? "GET").toUpperCase();
  const credentials =
    init?.credentials ?? request?.credentials ?? "same-origin";
  let url: URL;
  try {
    url = new URL(request?.url ?? String(input), consoleOrigin);
  } catch {
    return [input, init];
  }
  if (
    method !== "GET" ||
    credentials !== "omit" ||
    url.origin !== consoleOrigin ||
    url.search !== "" ||
    url.hash !== "" ||
    !GOVERNED_RRD_PATH.test(url.pathname)
  ) {
    return [input, init];
  }
  if (request) {
    return [
      new Request(request, {
        ...init,
        credentials: "same-origin",
      }),
      undefined,
    ];
  }
  return [
    input,
    {
      ...init,
      credentials: "same-origin",
    },
  ];
}

/**
 * Rerun 0.35 deliberately omits credentials on HTTP RRD receivers. Veoveo's
 * bounded live receiver and finite producer Blueprint are same-origin Console
 * resources, so their existing
 * HttpOnly session cookie must be restored at the browser Fetch boundary.
 *
 * The adapter is reversible and exact-path only. It cannot add credentials to
 * Redap, arbitrary HTTP sources, legacy archive routes, or cross-origin URLs.
 */
export function installConsoleRecordingRrdFetch(
  onLiveResponseEnded?: (url: string) => void
): () => void {
  if (installation) {
    installation.consumers += 1;
    if (onLiveResponseEnded) installation.liveEndObservers.add(onLiveResponseEnded);
    return releaseOnce(onLiveResponseEnded);
  }

  const original = globalThis.fetch;
  const origin = globalThis.location.origin;
  const liveEndObservers = new Set<(url: string) => void>();
  if (onLiveResponseEnded) liveEndObservers.add(onLiveResponseEnded);
  const adapted: typeof globalThis.fetch = async (input, init) => {
    const [authorizedInput, authorizedInit] =
      authorizeConsoleRecordingRrdFetch(input, init, origin);
    const response = await original.call(globalThis, authorizedInput, authorizedInit);
    let requestUrl: URL;
    try {
      requestUrl = new URL(
        authorizedInput instanceof Request
          ? authorizedInput.url
          : String(authorizedInput),
        origin
      );
    } catch {
      return response;
    }
    if (!isConsoleRecordingLiveRrdUrl(requestUrl, origin) || !response.ok || !response.body) {
      return response;
    }
    return observeRecordingLiveResponseEnd(response, () => {
      for (const observer of liveEndObservers) observer(requestUrl.toString());
    });
  };
  installation = {
    consumers: 1,
    liveEndObservers,
    original,
    adapted,
  };
  globalThis.fetch = adapted;
  return releaseOnce(onLiveResponseEnded);
}

function releaseOnce(observer?: (url: string) => void): () => void {
  let released = false;
  return () => {
    if (released) return;
    released = true;
    const current = installation;
    if (!current) return;
    if (observer) current.liveEndObservers.delete(observer);
    current.consumers -= 1;
    if (current.consumers > 0) return;
    if (globalThis.fetch === current.adapted) {
      globalThis.fetch = current.original;
    }
    installation = undefined;
  };
}

function isConsoleRecordingLiveRrdUrl(url: URL, consoleOrigin: string): boolean {
  return (
    url.origin === consoleOrigin &&
    url.search === "" &&
    url.hash === "" &&
    LIVE_RRD_PATH.test(url.pathname)
  );
}

export function observeRecordingLiveResponseEnd(
  response: Response,
  onEnd: () => void
): Response {
  const reader = response.body!.getReader();
  let ended = false;
  const notifyEnd = () => {
    if (ended) return;
    ended = true;
    onEnd();
  };
  const body = new ReadableStream<Uint8Array>({
    async pull(controller) {
      try {
        const next = await reader.read();
        if (next.done) {
          controller.close();
          notifyEnd();
        } else {
          controller.enqueue(next.value);
        }
      } catch (cause) {
        notifyEnd();
        controller.error(cause);
      }
    },
    cancel(reason) {
      return reader.cancel(reason);
    },
  });
  return new Response(body, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
  });
}
