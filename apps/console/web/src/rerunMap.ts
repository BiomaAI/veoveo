import { authenticationRequired } from "./auth.ts";

export interface RerunMapViewerOptions {
  mapbox_access_token?: string;
}

type RerunMapConfig =
  | { provider: "openStreetMap" }
  | { provider: "mapbox"; accessToken: string };

let optionsPromise: Promise<RerunMapViewerOptions> | undefined;

export function loadRerunMapViewerOptions(): Promise<RerunMapViewerOptions> {
  optionsPromise ??= resolveRerunMapViewerOptions(window.fetch.bind(window));
  return optionsPromise;
}

export async function resolveRerunMapViewerOptions(
  fetcher: typeof fetch,
): Promise<RerunMapViewerOptions> {
  const response = await fetcher("/console/api/viewer/rerun/map-config", {
    credentials: "same-origin",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  if (response.status === 401) authenticationRequired();
  if (!response.ok) {
    throw new Error(`Rerun map configuration returned ${response.status}`);
  }
  const config = decodeRerunMapConfig(await response.json());
  if (config.provider === "openStreetMap") return {};

  await validateMapboxAccessToken(fetcher, config.accessToken);
  return { mapbox_access_token: config.accessToken };
}

function decodeRerunMapConfig(value: unknown): RerunMapConfig {
  if (!value || typeof value !== "object") {
    throw new Error("Rerun map configuration is malformed");
  }
  const candidate = value as { provider?: unknown; accessToken?: unknown };
  if (candidate.provider === "openStreetMap" && candidate.accessToken === undefined) {
    return { provider: "openStreetMap" };
  }
  if (
    candidate.provider === "mapbox" &&
    typeof candidate.accessToken === "string" &&
    isBrowserSafeMapboxToken(candidate.accessToken)
  ) {
    return { provider: "mapbox", accessToken: candidate.accessToken };
  }
  throw new Error(
    candidate.provider === "mapbox"
      ? "The installation supplied an invalid browser-safe Mapbox token"
      : "Rerun map configuration selected an unsupported provider",
  );
}

function isBrowserSafeMapboxToken(token: string): boolean {
  return token.length >= 8 && token.length <= 4096 && /^pk\.[A-Za-z0-9._-]+$/.test(token);
}

async function validateMapboxAccessToken(fetcher: typeof fetch, accessToken: string): Promise<void> {
  const validationUrl = new URL("https://api.mapbox.com/styles/v1/mapbox/streets-v12");
  validationUrl.searchParams.set("access_token", accessToken);
  let response: Response;
  try {
    response = await fetcher(validationUrl, {
      credentials: "omit",
      cache: "no-store",
      headers: { Accept: "application/json" },
      referrerPolicy: "origin",
    });
  } catch {
    throw new Error("Mapbox token validation could not reach the configured map provider");
  }
  if (response.ok) return;
  if (response.status === 401) {
    throw new Error("Mapbox rejected the installation token as invalid or expired");
  }
  if (response.status === 403) {
    throw new Error(
      "Mapbox denied the installation token; verify its scopes and allowed Console origins",
    );
  }
  throw new Error(`Mapbox token validation returned ${response.status}`);
}
