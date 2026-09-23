/**
 * User-facing text for failed Console requests. Each message says what went
 * wrong and what the person can do next, without HTTP codes or internal terms.
 */
export interface RequestSubject {
  /** Verb phrase for the attempted action, e.g. "load the cluster inventory". */
  action: string;
  /** Noun phrase for the requested item, e.g. "This artifact". */
  thing?: string;
}

export function httpErrorMessage(status: number, subject: RequestSubject): string {
  if (status === 401) return "Your session has ended. Sign in again to continue.";
  if (status === 403) return forbiddenMessage(subject.action);
  if (status === 404) return `${subject.thing ?? "This item"} was not found. It may have been removed.`;
  if (status === 409) return `${subject.thing ?? "This item"} changed since you loaded it. Refresh and try again.`;
  if (status === 413) return "The request is too large to send.";
  if (status === 429) return "Too many requests right now. Wait a moment and try again.";
  return unavailableMessage(subject.action);
}

export function forbiddenMessage(action: string): string {
  return `You don't have permission to ${action}. Ask an administrator for access.`;
}

export function unavailableMessage(action: string): string {
  return `Veoveo couldn't ${action}. Try again; if it keeps failing, contact your installation operator.`;
}

export const unexpectedResponseMessage = "Veoveo returned an unexpected response. Refresh the page and try again.";

export const sessionNotReadyMessage = "Your session isn't ready yet. Reload the page and try again.";
