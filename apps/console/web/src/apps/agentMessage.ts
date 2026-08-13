import type { AppDescriptor } from "../types";

export const AGENT_MESSAGE_METHOD = "veoveo/agents/message";

export interface AppAgentMessageRequest {
  agentId: string;
  requestId: string;
  message: string;
}

export function appAgentMessageRequest(
  app: Pick<AppDescriptor, "agentMessageTargets">,
  value: unknown,
): AppAgentMessageRequest | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const params = value as Record<string, unknown>;
  if (Object.keys(params).sort().join(",") !== "agentId,message,requestId") return undefined;
  const { agentId, requestId, message } = params;
  if (
    typeof agentId !== "string" ||
    !app.agentMessageTargets.includes(agentId) ||
    typeof requestId !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
      requestId,
    ) ||
    typeof message !== "string" ||
    message.trim().length === 0 ||
    new TextEncoder().encode(message).length > 16 * 1024
  ) {
    return undefined;
  }
  return { agentId, requestId, message };
}
