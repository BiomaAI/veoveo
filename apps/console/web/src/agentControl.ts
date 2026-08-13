import type { AgentSummary } from "./types";

export type AgentDisplayState = AgentSummary["state"] | "offline";

export function agentElicitationsPath(agentKey: string): string {
  return `agents/${encodeURIComponent(agentKey)}/elicitations`;
}

export function agentElicitationsApiPath(agentKey: string): string {
  return `/console/api/${agentElicitationsPath(agentKey)}`;
}

export function agentElicitationDecisionPath(
  agentKey: string,
  elicitationId: string,
): string {
  return `${agentElicitationsPath(agentKey)}/${encodeURIComponent(elicitationId)}/decision`;
}

/** Project durable episode state through the exact runner-lease deadline. */
export function agentDisplayState(
  agent: Pick<AgentSummary, "state" | "runnerLeaseExpiresAt">,
  nowMs = Date.now(),
): AgentDisplayState {
  if (agent.state === "disabled" || agent.state === "failed") return agent.state;
  if (!agent.runnerLeaseExpiresAt) return "offline";
  const expiresAt = Date.parse(agent.runnerLeaseExpiresAt);
  return Number.isFinite(expiresAt) && expiresAt > nowMs ? agent.state : "offline";
}

/** Generate a client-owned UUIDv7 for retry-safe agent control operations. */
export function uuidV7(
  timestampMs = Date.now(),
  randomBytes?: Uint8Array,
): string {
  if (!Number.isSafeInteger(timestampMs) || timestampMs < 0 || timestampMs >= 2 ** 48) {
    throw new Error("UUIDv7 timestamp is outside the 48-bit Unix millisecond range");
  }
  if (randomBytes !== undefined && randomBytes.length !== 16) {
    throw new Error("UUIDv7 entropy must contain exactly 16 bytes");
  }
  const bytes = randomBytes === undefined
    ? crypto.getRandomValues(new Uint8Array(16))
    : Uint8Array.from(randomBytes);
  let timestamp = BigInt(timestampMs);
  for (let index = 5; index >= 0; index -= 1) {
    bytes[index] = Number(timestamp & 0xffn);
    timestamp >>= 8n;
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
