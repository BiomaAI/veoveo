import { isJSONRPCErrorResponse } from "@modelcontextprotocol/client";
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
// Native deferred input requests carry method/params, without an RPC id.
export type InputRequests = Record<string, Pick<JsonRpcRequest, "method" | "params">>;

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

// The HTTP edge carries the SDK's standard error unchanged only for a definitive
// protocol response. Host errors and transport uncertainty do not use this class.
export type AppProtocolErrorData = import("@modelcontextprotocol/client").JSONRPCErrorResponse["error"];
export class AppProtocolError extends Error {
  readonly #protocolError: AppProtocolErrorData;
  constructor(error: AppProtocolErrorData) {
    super("The MCP server refused this request.");
    this.name = "AppProtocolError";
    this.#protocolError = error;
  }
  get protocolError(): AppProtocolErrorData { return this.#protocolError; }
}

export function appRequestError(error: unknown): AppProtocolErrorData {
  return error instanceof AppProtocolError
    ? error.protocolError
    : { code: INTERNAL_ERROR, message: "The App host could not complete this request." };
}

export function admitAppProtocolFailure(value: unknown): AppProtocolError | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return;
  const envelope = value as Record<string, unknown>;
  if (envelope.kind !== "mcpProtocol" || Object.keys(envelope).some(key => key !== "kind" && key !== "error")) return;
  if (typeof envelope.error !== "object" || envelope.error === null || Array.isArray(envelope.error)) return;
  if (!isJSONRPCErrorResponse({jsonrpc: "2.0", id: 0, error: envelope.error})) return;
  const error = envelope.error as AppProtocolErrorData;
  if (!Number.isSafeInteger(error.code) || error.code < -2147483648 || error.code > 2147483647 || typeof error.message !== "string" ||
      Object.keys(error).some(key => key !== "code" && key !== "message" && key !== "data")) return;
  return new AppProtocolError(error);
}

async function boundedAppFailure(response: Response): Promise<unknown> {
  const cap = 2 * 1024 * 1024;
  if (response.headers.get("content-type")?.split(";")[0]?.trim().toLowerCase() !== "application/json" || !response.body) {
    try { await response.body?.cancel(); } catch { /* Invalid error body remains generic. */ }
    return undefined;
  }
  const length = response.headers.get("content-length");
  if (length !== null && (!/^\d+$/.test(length) || !Number.isSafeInteger(Number(length)) || Number(length) > cap)) {
    try { await response.body.cancel(); } catch { /* Invalid error body remains generic. */ }
    return undefined;
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const next = await reader.read();
      if (next.done) break;
      total += next.value.byteLength;
      if (total > cap) { await reader.cancel(); return undefined; }
      chunks.push(next.value);
    }
    const body = new Uint8Array(total);
    let offset = 0;
    for (const chunk of chunks) { body.set(chunk, offset); offset += chunk.byteLength; }
    return JSON.parse(new TextDecoder("utf-8", {fatal:true}).decode(body));
  } catch { return undefined; }
  finally { reader.releaseLock(); }
}

export async function readAppProtocolFailure(response: Response): Promise<AppProtocolError | undefined> {
  if (response.status !== 422) return undefined;
  return admitAppProtocolFailure(await boundedAppFailure(response));
}
