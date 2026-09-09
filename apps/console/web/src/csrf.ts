/** Session-local state only. Never persisted with uploads or App state. */
export const consoleSession: { csrfToken?: string } = {};
export function consoleCsrfToken(): string | undefined { return consoleSession.csrfToken; }
export function acceptConsoleCsrfToken(token: string | null): void {
  if (token) consoleSession.csrfToken = token;
}
