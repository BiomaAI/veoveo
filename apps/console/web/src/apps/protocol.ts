import type {
  CallToolResult,
  JSONRPCMessage,
  ReadResourceResult,
  Result,
  Transport,
} from "@modelcontextprotocol/client";

export type {
  CallToolResult,
  JSONRPCMessage,
  ReadResourceResult,
  Result,
  Transport,
};

export const INVALID_PARAMS = -32602;
export const INTERNAL_ERROR = -32603;

export interface JsonRpcRequest {
  jsonrpc: "2.0";
  id: string | number;
  method: string;
  params?: Record<string, unknown>;
}

export function isJsonRpcRequest(value: unknown): value is JsonRpcRequest {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const request = value as Partial<JsonRpcRequest>;
  return (
    request.jsonrpc === "2.0" &&
    (typeof request.id === "string" || typeof request.id === "number") &&
    typeof request.method === "string"
  );
}

export type InputResponses = Record<string, unknown>;
export type InputRequests = Record<string, JsonRpcRequest>;

export interface InputRequiredResult extends Result {
  resultType: "input_required";
  inputRequests?: InputRequests;
  requestState?: string;
}

export interface TaskSeedResult extends Result {
  resultType: "task";
  taskId: string;
  status: "working" | "input_required" | "completed" | "failed" | "cancelled";
  statusMessage?: string;
  createdAt: string;
  lastUpdatedAt: string;
  ttlMs: number | null;
  pollIntervalMs?: number;
}

export interface TaskDetailResult extends Result {
  resultType: "complete";
  taskId: string;
  status: "working" | "input_required" | "completed" | "failed" | "cancelled";
  statusMessage?: string;
  createdAt: string;
  lastUpdatedAt: string;
  ttlMs: number | null;
  pollIntervalMs?: number;
  inputRequests?: InputRequests;
  result?: Record<string, unknown>;
  error?: Record<string, unknown>;
}

export interface TaskAckResult extends Result {
  resultType: "complete";
}

export type AppToolResult = CallToolResult | InputRequiredResult | TaskSeedResult;

export interface AppToolRequestExtras {
  inputResponses?: InputResponses;
  requestState?: string;
}

export interface AppTransport extends Transport {
  send(message: JSONRPCMessage): Promise<void>;
}

export { type ReadResourceResult as AppReadResourceResult };
