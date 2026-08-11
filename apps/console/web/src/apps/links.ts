import type { AppDescriptor } from "../types";

export type PlatformAppLink = "agents" | "recordings";

export type ResolvedAppLink =
  | { kind: "app"; app: AppDescriptor }
  | { kind: "platform"; view: PlatformAppLink };

const PLATFORM_LINKS: Readonly<Record<string, PlatformAppLink>> = {
  "veoveo-console://agents": "agents",
  "veoveo-console://recordings": "recordings",
};

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
