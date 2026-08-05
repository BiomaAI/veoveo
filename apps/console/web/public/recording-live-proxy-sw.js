const READ_MESSAGES_PATH =
  "/rerun.sdk_comms.v1alpha1.MessageProxyService/ReadMessages";
const UUID_V7 =
  "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const LIVE_PROXY_PATH = new RegExp(
  `^/console/api/recordings/${UUID_V7}/live/proxy$`
);

/** @type {Map<string, string>} */
const routesByClient = new Map();

self.addEventListener("install", (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener("message", (event) => {
  if (!event.source || event.data?.kind !== "veoveo-rerun-live-route") return;
  const clientId = event.source.id;
  const route = event.data.route;
  let accepted = false;
  if (route === null) {
    routesByClient.delete(clientId);
    accepted = true;
  } else if (typeof route === "string") {
    try {
      const url = new URL(route, self.location.origin);
      if (
        url.origin === self.location.origin &&
        url.search === "" &&
        url.hash === "" &&
        LIVE_PROXY_PATH.test(url.pathname)
      ) {
        routesByClient.set(clientId, url.toString());
        accepted = true;
      }
    } catch {
      accepted = false;
    }
  }
  event.ports[0]?.postMessage({ accepted });
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (
    request.method !== "POST" ||
    url.origin !== self.location.origin ||
    url.pathname !== READ_MESSAGES_PATH ||
    url.search !== "" ||
    url.hash !== ""
  ) {
    return;
  }
  event.respondWith(forwardReadMessages(event.clientId, request));
});

async function forwardReadMessages(clientId, request) {
  const route = routesByClient.get(clientId);
  if (!route) {
    return new Response("No governed live recording route is active for this client.", {
      status: 409,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  }
  const body = await request.arrayBuffer();
  return fetch(route, {
    method: "POST",
    headers: request.headers,
    body,
    credentials: "same-origin",
    cache: "no-store",
    redirect: "error",
    signal: request.signal,
  });
}
