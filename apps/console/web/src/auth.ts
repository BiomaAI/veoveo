let loginRedirectStarted = false;

function currentConsoleReturnPath(): string {
  return `${window.location.pathname}${window.location.search}${window.location.hash}`;
}

export function consoleLoginPath(returnPath: string): string {
  const query = new URLSearchParams({ return_to: returnPath });
  return `/auth/login?${query.toString()}`;
}

export class AuthenticationRequiredError extends Error {
  constructor() {
    super("Authentication required");
    this.name = "AuthenticationRequiredError";
  }
}

export function redirectToLogin(
  navigate: (path: string) => void = (path) => window.location.replace(path),
  returnPath: string = currentConsoleReturnPath(),
): boolean {
  if (loginRedirectStarted) {
    return false;
  }
  loginRedirectStarted = true;
  navigate(consoleLoginPath(returnPath));
  return true;
}

export function authenticationRequired(): never {
  redirectToLogin();
  throw new AuthenticationRequiredError();
}
