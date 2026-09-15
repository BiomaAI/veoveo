import { browserLoginRoot } from "./browserApp.ts";
let loginRedirectStarted = false;

function currentBrowserReturnPath(): string {
  return `${window.location.pathname}${window.location.search}${window.location.hash}`;
}

export function browserLoginPath(returnPath: string): string {
  const query = new URLSearchParams({ return_to: returnPath });
  return `${browserLoginRoot()}?${query.toString()}`;
}

export class AuthenticationRequiredError extends Error {
  constructor() {
    super("Authentication required");
    this.name = "AuthenticationRequiredError";
  }
}

export function redirectToLogin(
  navigate: (path: string) => void = (path) => window.location.replace(path),
  returnPath: string = currentBrowserReturnPath(),
): boolean {
  if (loginRedirectStarted) {
    return false;
  }
  loginRedirectStarted = true;
  navigate(browserLoginPath(returnPath));
  return true;
}

export function authenticationRequired(): never {
  redirectToLogin();
  throw new AuthenticationRequiredError();
}
