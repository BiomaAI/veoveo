let loginRedirectStarted = false;

export class AuthenticationRequiredError extends Error {
  constructor() {
    super("Authentication required");
    this.name = "AuthenticationRequiredError";
  }
}

export function redirectToLogin(
  navigate: (path: string) => void = (path) => window.location.replace(path),
): boolean {
  if (loginRedirectStarted) {
    return false;
  }
  loginRedirectStarted = true;
  navigate("/auth/login");
  return true;
}

export function authenticationRequired(): never {
  redirectToLogin();
  throw new AuthenticationRequiredError();
}
