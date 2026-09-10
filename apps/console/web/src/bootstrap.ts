import { useQuery } from "@tanstack/react-query";
import { consoleJson } from "./consoleHttp";
import { parseConsoleBootstrap } from "./generatedContracts";
import type { ConsoleBootstrap } from "./generated/console";
import { demoSnapshot } from "./demo";

export function consoleIdentityScope(value: ConsoleBootstrap): string {
  return JSON.stringify([
    value.profile,
    value.session.tenantId,
    value.session.actorId,
    value.session.workContext,
  ]);
}
export function useConsoleBootstrap() {
  return useQuery({
    queryKey: ["console-session"],
    queryFn: async ({ signal }) => {
      if (import.meta.env.VITE_DEMO_DATA === "true") {
        return parseConsoleBootstrap({
          profile: "demo",
          canReadInstallation: true,
          installation: demoSnapshot.installation,
          session: demoSnapshot.session,
        });
      }
      return parseConsoleBootstrap(await consoleJson("session", undefined, signal));
    },
    staleTime: 30_000,
    refetchOnWindowFocus: true,
  });
}
