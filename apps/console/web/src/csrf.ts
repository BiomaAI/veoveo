/** Session-local state only. Never persisted with uploads or App state. */
export const browserSession: { csrfToken?: string } = {};
export function browserCsrfToken(): string | undefined { return browserSession.csrfToken; }
export function acceptBrowserCsrfToken(token: string | null): void {
  if (token) browserSession.csrfToken = token;
}
