import type { AppCatalogDegradation, AppDescriptor } from "../types";

export function unavailableAppServers(
  apps: AppDescriptor[],
  degradations: AppCatalogDegradation[],
): string[] {
  const availableServers = new Set(apps.map((app) => app.server));
  return [...new Set(degradations.map((failure) => failure.server))]
    .filter((server) => !availableServers.has(server))
    .sort();
}

export function appServerTitle(server: string): string {
  return server
    .split("-")
    .filter(Boolean)
    .map((part) =>
      ["gis", "mcp", "uav"].includes(part)
        ? part.toUpperCase()
        : part.charAt(0).toUpperCase() + part.slice(1),
    )
    .join(" ");
}
