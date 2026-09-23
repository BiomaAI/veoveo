import type { AppCatalogDegradation, AppDescriptor } from "../types";

export interface AppServerGroup {
  server: string;
  title: string;
  apps: AppDescriptor[];
  unavailable: boolean;
  discovering: boolean;
}

export function unavailableAppServers(
  apps: AppDescriptor[],
  degradations: AppCatalogDegradation[],
): string[] {
  const availableServers = new Set(apps.map((app) => app.server));
  return [...new Set(degradations.filter(failure => failure.code !== "discovery_pending").map((failure) => failure.server))]
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

/**
 * The Console owns navigation composition. MCP servers own only their local
 * App resources; this deterministic projection supplies the visible server
 * namespace without adding Console-specific metadata to the MCP contract.
 */
export function groupAppsByServer(
  apps: AppDescriptor[],
  degradations: AppCatalogDegradation[],
): AppServerGroup[] {
  const groups = new Map<string, AppServerGroup>();
  const seen = new Set<string>();
  for (const app of apps) {
    // A paged catalog can briefly repeat a resource while discovery changes.
    // React keys must be unique even during that intermediate snapshot.
    if (seen.has(app.resourceUri)) continue;
    seen.add(app.resourceUri);
    const group = groups.get(app.server) ?? {
      server: app.server,
      title: appServerTitle(app.server),
      apps: [],
      unavailable: false,
      discovering: false,
    };
    group.apps.push(app);
    groups.set(app.server, group);
  }
  for (const server of unavailableAppServers(apps, degradations)) {
    groups.set(server, {
      server,
      title: appServerTitle(server),
      apps: [],
      unavailable: true,
      discovering: false,
    });
  }
  for (const failure of degradations) {
    if (!groups.has(failure.server)) {
      groups.set(failure.server, { server: failure.server, title: appServerTitle(failure.server), apps: [], unavailable: false, discovering: true });
    }
  }
  return [...groups.values()]
    .map((group) => ({
      ...group,
      apps: [...group.apps].sort((left, right) =>
        (left.title ?? left.name).localeCompare(right.title ?? right.name),
      ),
    }))
    .sort((left, right) => left.title.localeCompare(right.title));
}

export function namespacedAppTitle(app: AppDescriptor): string {
  return `${appServerTitle(app.server)} / ${app.title ?? app.name}`;
}
