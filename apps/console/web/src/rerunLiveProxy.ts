const UUID_V7 =
  "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const LIVE_PROXY_PATH = new RegExp(
  `^/console/api/recordings/${UUID_V7}/live/proxy$`
);
const ROUTE_MESSAGE_KIND = "veoveo-rerun-live-route";

let controllerPromise: Promise<ServiceWorker> | undefined;

export function consoleRerunMessageProxyUri(location: Location = window.location): string {
  return `rerun+${location.protocol}//${location.host}/proxy`;
}

export function validateConsoleRerunLiveProxyRoute(
  route: string,
  consoleOrigin: string = window.location.origin
): string {
  const url = new URL(route, consoleOrigin);
  if (
    url.origin !== consoleOrigin ||
    url.search !== "" ||
    url.hash !== "" ||
    !LIVE_PROXY_PATH.test(url.pathname)
  ) {
    throw new Error("Live recording route is outside the governed Console boundary.");
  }
  return url.toString();
}

export async function setConsoleRerunLiveProxyRoute(route: string): Promise<void> {
  const canonical = validateConsoleRerunLiveProxyRoute(route);
  const controller = await controlledServiceWorker();
  await postRoute(controller, canonical);
}

export async function clearConsoleRerunLiveProxyRoute(): Promise<void> {
  if (!("serviceWorker" in navigator)) return;
  const controller = navigator.serviceWorker.controller;
  if (!controller) return;
  await postRoute(controller, null);
}

async function controlledServiceWorker(): Promise<ServiceWorker> {
  if (!("serviceWorker" in navigator)) {
    throw new Error("This browser cannot route governed Rerun live playback.");
  }
  if (!controllerPromise) {
    controllerPromise = (async () => {
      const registration = await navigator.serviceWorker.register(
        "/console/recording-live-proxy-sw.js",
        { scope: "/console/", updateViaCache: "none" }
      );
      const pending = registration.installing ?? registration.waiting;
      if (pending && pending.state !== "activated") {
        await new Promise<void>((resolve, reject) => {
          const activated = () => {
            if (pending.state === "redundant") {
              pending.removeEventListener("statechange", activated);
              reject(new Error("The governed Rerun routing worker did not activate."));
              return;
            }
            if (pending.state !== "activated") return;
            pending.removeEventListener("statechange", activated);
            resolve();
          };
          pending.addEventListener("statechange", activated);
        });
      }
      await navigator.serviceWorker.ready;
      if (navigator.serviceWorker.controller) {
        return navigator.serviceWorker.controller;
      }
      return new Promise<ServiceWorker>((resolve) => {
        navigator.serviceWorker.addEventListener(
          "controllerchange",
          () => resolve(navigator.serviceWorker.controller!),
          { once: true }
        );
      });
    })().catch((cause) => {
      controllerPromise = undefined;
      throw cause;
    });
  }
  return controllerPromise;
}

async function postRoute(controller: ServiceWorker, route: string | null): Promise<void> {
  const channel = new MessageChannel();
  const acknowledged = new Promise<boolean>((resolve) => {
    channel.port1.onmessage = (event: MessageEvent<{ accepted?: boolean }>) => {
      resolve(event.data?.accepted === true);
    };
  });
  controller.postMessage({ kind: ROUTE_MESSAGE_KIND, route }, [channel.port2]);
  if (!(await acknowledged)) {
    throw new Error("The Console rejected the governed Rerun live route.");
  }
}
