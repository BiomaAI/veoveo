import { authenticationRequired } from "./auth.ts";
import type { AppDescriptor } from "./types.ts";

export interface StandaloneBootstrap {
  app: AppDescriptor;
  csrfToken: string;
}

export async function requestStandaloneBootstrap(
  pathname: string,
  fetchBootstrap: typeof fetch = fetch,
): Promise<StandaloneBootstrap> {
  const response = await fetchBootstrap(pathname, {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
  });
  if (response.status === 401) authenticationRequired();
  if (response.status === 404) {
    throw new Error("This MCP App is not available to the current session.");
  }
  if (!response.ok) {
    throw new Error(`Standalone App bootstrap returned ${response.status}`);
  }
  const csrfToken = response.headers.get("x-veoveo-csrf-token");
  if (!csrfToken) throw new Error("Standalone App bootstrap omitted CSRF settlement");
  const app = (await response.json()) as AppDescriptor;
  return { app, csrfToken };
}
