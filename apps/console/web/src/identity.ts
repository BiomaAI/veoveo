import type { InstallationSnapshot } from "./types";

export type IdentityDirectory = Pick<InstallationSnapshot, "principals" | "session">;

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export function identityLabel(identity: string | undefined, directory: IdentityDirectory): string {
  const value = identity?.trim();
  if (!value) return "-";

  if (value === directory.session.principalId || value === directory.session.actorId) {
    return readableName(directory.session.displayName, value);
  }
  const principal = directory.principals.find((entry) => entry.id === value);
  return principal ? readableName(principal.displayName, value) : compactIdentityLabel(value);
}

function readableName(displayName: string, identity: string): string {
  const candidate = displayName.trim();
  if (
    candidate
    && candidate !== identity
    && !candidate.includes("://")
    && candidate !== identitySegment(identity)
  ) {
    return candidate;
  }
  return compactIdentityLabel(identity);
}

export function compactIdentityLabel(identity: string): string {
  const segment = identitySegment(identity);
  if (UUID.test(segment)) return `Principal ${segment.slice(0, 8)}`;
  if (segment.includes("@")) return segment.split("@", 1)[0] || "Principal";
  return segment || "Principal";
}

function identitySegment(identity: string): string {
  return identity
    .split(/[/#]/)
    .filter((segment) => segment.trim())
    .at(-1)
    ?.trim() ?? identity.trim();
}
