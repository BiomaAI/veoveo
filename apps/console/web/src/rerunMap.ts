import { authenticationRequired } from "./auth.ts";

export interface RerunMapViewerOptions {
  mapbox_access_token?: string;
}

type RerunMapConfig =
  | { provider: "openStreetMap" }
  | { provider: "mapbox"; accessToken?: string; diagnostic?: string };

export interface RerunMapViewerSetup {
  provider: "openStreetMap" | "mapbox";
  options: RerunMapViewerOptions;
  mapError?: string;
}

let optionsPromise: Promise<RerunMapViewerSetup> | undefined;

export function loadRerunMapViewerOptions(): Promise<RerunMapViewerSetup> {
  optionsPromise ??= resolveRerunMapViewerOptions(window.fetch.bind(window));
  return optionsPromise;
}

export async function resolveRerunMapViewerOptions(
  fetcher: typeof fetch,
): Promise<RerunMapViewerSetup> {
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
  if (config.provider === "openStreetMap") {
    return { provider: "openStreetMap", options: {} };
  }
  if (!config.accessToken) {
    return {
      provider: "mapbox",
      options: {},
      mapError:
        config.diagnostic ?? "Mapbox is selected, but its installation token is unavailable",
    };
  }

  try {
    await validateMapboxAccessToken(fetcher, config.accessToken);
    return {
      provider: "mapbox",
      options: { mapbox_access_token: config.accessToken },
    };
  } catch (cause) {
    return {
      provider: "mapbox",
      options: {},
      mapError: cause instanceof Error ? cause.message : "Mapbox validation failed",
    };
  }
}

function decodeRerunMapConfig(value: unknown): RerunMapConfig {
  if (!value || typeof value !== "object") {
    throw new Error("Rerun map configuration is malformed");
  }
  const candidate = value as {
    provider?: unknown;
    accessToken?: unknown;
    diagnostic?: unknown;
  };
  if (candidate.provider === "openStreetMap" && candidate.accessToken === undefined) {
    return { provider: "openStreetMap" };
  }
  if (
    candidate.provider === "mapbox" &&
    (candidate.accessToken === undefined ||
      (typeof candidate.accessToken === "string" &&
        isBrowserSafeMapboxToken(candidate.accessToken))) &&
    (candidate.diagnostic === undefined || typeof candidate.diagnostic === "string")
  ) {
    return {
      provider: "mapbox",
      accessToken: candidate.accessToken as string | undefined,
      diagnostic: candidate.diagnostic as string | undefined,
    };
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

export function mapProviderCompatibilityError(
  installationProvider: "openStreetMap" | "mapbox",
  blueprintProvider: "none" | "openStreetMap" | "mapbox" | "mixed" | undefined,
): string | undefined {
  if (blueprintProvider === "mixed") {
    return "The producer Blueprint mixes map-provider families; every map view must use the installation-selected provider.";
  }
  if (installationProvider === "mapbox" && blueprintProvider !== "mapbox") {
    return "Mapbox is selected, but the producer Blueprint does not select a Mapbox background.";
  }
  if (installationProvider === "openStreetMap" && blueprintProvider === "mapbox") {
    return "The producer Blueprint selects Mapbox, but this installation selects OpenStreetMap.";
  }
  return undefined;
}
