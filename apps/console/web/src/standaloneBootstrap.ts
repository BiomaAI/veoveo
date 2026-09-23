import { authenticationRequired } from "./auth.ts";
import { httpErrorMessage } from "./httpMessages.ts";
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
    throw new Error("This MCP App isn't available to your account. Ask an administrator for access.");
  }
  if (!response.ok) {
    throw new Error(httpErrorMessage(response.status, { action: "open this MCP App" }));
  }
  const csrfToken = response.headers.get("x-veoveo-csrf-token");
  if (!csrfToken) throw new Error("The app couldn't start a secure session (missing CSRF token). Reload the page.");
  const app = (await response.json()) as AppDescriptor;
  return { app, csrfToken };
}
