import type { AppDescriptor } from "../types";

export type PlatformAppLink = "agents" | "recordings";

export type ResolvedAppLink =
  | { kind: "app"; app: AppDescriptor }
  | { kind: "platform"; view: PlatformAppLink };

const PLATFORM_LINKS: Readonly<Record<string, PlatformAppLink>> = {
  "veoveo-console://agents": "agents",
  "veoveo-console://recordings": "recordings",
};

/** Browser navigation is a catalog projection, not an MCP resource URI. */
export function appRouteKey(app: AppDescriptor): string {
  return app.resourceUri.replace(/^ui:\/\//, "").replace(/\.html$/i, "");
}

export function consoleAppRoute(app: AppDescriptor): string {
  return `#/apps/${appRouteKey(app)}`;
}

export function appForRoute(key: string | undefined, apps: readonly AppDescriptor[]): AppDescriptor | undefined {
  const matches = apps.filter(app => appRouteKey(app) === key);
  return matches.length === 1 ? matches[0] : undefined;
}

/** Resolve only exact discovered Apps or explicitly supported platform views. */
export function resolveAppLink(
  value: string,
  apps: readonly AppDescriptor[],
): ResolvedAppLink | undefined {
  const platform = PLATFORM_LINKS[value];
  if (platform !== undefined) return { kind: "platform", view: platform };
  if (!value.startsWith("ui://") || value.includes("..")) return undefined;
  const app = apps.find((candidate) => candidate.resourceUri === value);
  return app === undefined ? undefined : { kind: "app", app };
}
