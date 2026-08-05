const UUID_V7 =
  "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const BLUEPRINT_RRD_PATH = new RegExp(
  `^/console/api/recordings/${UUID_V7}/blueprints/[1-9][0-9]*/data\\.rrd$`
);

let installation:
  | { consumers: number; original: typeof globalThis.fetch; adapted: typeof globalThis.fetch }
  | undefined;

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
    BLUEPRINT_RRD_PATH.test(url.pathname)
  );
}

export function authorizeConsoleRecordingRrdFetch(
  input: RequestInfo | URL,
  init: RequestInit | undefined,
  consoleOrigin: string
): readonly [RequestInfo | URL, RequestInit | undefined] {
  const request = input instanceof Request ? input : undefined;
  const method = (init?.method ?? request?.method ?? "GET").toUpperCase();
  const credentials = init?.credentials ?? request?.credentials ?? "same-origin";
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
    !BLUEPRINT_RRD_PATH.test(url.pathname)
  ) {
    return [input, init];
  }
  if (request) {
    return [new Request(request, { ...init, credentials: "same-origin" }), undefined];
  }
  return [input, { ...init, credentials: "same-origin" }];
}

export function installConsoleRecordingRrdFetch(): () => void {
  if (installation) {
    installation.consumers += 1;
    return releaseOnce();
  }
  const original = globalThis.fetch;
  const origin = globalThis.location.origin;
  const adapted: typeof globalThis.fetch = async (input, init) => {
    const [authorizedInput, authorizedInit] = authorizeConsoleRecordingRrdFetch(
      input,
      init,
      origin
    );
    return original.call(globalThis, authorizedInput, authorizedInit);
  };
  installation = { consumers: 1, original, adapted };
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
    if (globalThis.fetch === current.adapted) globalThis.fetch = current.original;
    installation = undefined;
  };
}
