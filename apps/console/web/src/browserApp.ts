/** Trusted entrypoint selection. Server-side cookies and profiles establish authority. */
export type BrowserApplication = "console" | "workspace";
let application: BrowserApplication | undefined;

export function configureBrowserApplication(value: BrowserApplication): void {
  if (application && application !== value) throw new Error("A browser application cannot change its session boundary.");
  application = value;
}

export function browserApplication(): BrowserApplication {
  if (!application) throw new Error("The browser application has not been initialized.");
  return application;
}

export function browserApiRoot(): "/console/api" | "/workspace/api" {
  return browserApplication() === "console" ? "/console/api" : "/workspace/api";
}

export function browserLoginRoot(): "/auth/login" | "/workspace/auth/login" {
  return browserApplication() === "console" ? "/auth/login" : "/workspace/auth/login";
}
