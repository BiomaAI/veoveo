import type {
  AppToolResult,
  CallToolResult,
  JSONRPCMessage,
  ReadResourceResult,
  Transport,
} from "./protocol.ts";
import { INTERNAL_ERROR, INVALID_PARAMS, isJsonRpcRequest } from "./protocol.ts";

const APPS_PROTOCOL_VERSION = "2026-01-26";

interface HostContext {
  theme: "light" | "dark";
  displayMode: "inline";
  availableDisplayModes: ["inline"];
  locale: string;
  platform: "web";
  containerDimensions: { width: number };
}

interface AppBridgeOptions {
  hostContext: HostContext;
}

interface ToolCallParams {
  name: string;
  arguments?: Record<string, unknown>;
  inputResponses?: Record<string, unknown>;
  requestState?: string;
}

type ToolCallHandler = (params: ToolCallParams) => Promise<AppToolResult>;
type ReadResourceHandler = (params: { uri: string }) => Promise<ReadResourceResult>;
type OpenLinkHandler = (params: { url: string }) => Promise<Record<string, unknown>>;
type DisplayModeHandler = () => Promise<{ mode: "inline" }>;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isJsonRpcMessage(value: unknown): value is JSONRPCMessage {
  if (!isRecord(value) || value.jsonrpc !== "2.0") return false;
  if (typeof value.method === "string") return true;
  return (
    (typeof value.id === "string" || typeof value.id === "number") &&
    ("result" in value || "error" in value)
  );
}

/** Browser transport for the MCP Apps iframe channel. */
export class PostMessageTransport implements Transport {
  onclose?: () => void;
  onerror?: (error: Error) => void;
  onmessage?: Transport["onmessage"];
  private started = false;

  constructor(
    private readonly eventTarget: Window,
    private readonly eventSource?: Window,
  ) {}

  private readonly receive = (event: MessageEvent<unknown>) => {
    if (this.eventSource && event.source !== this.eventSource) return;
    if (!isJsonRpcMessage(event.data)) {
      if (isRecord(event.data) && event.data.jsonrpc === "2.0") {
        this.onerror?.(new Error("invalid JSON-RPC message received from MCP App"));
      }
      return;
    }
    this.onmessage?.(event.data);
  };

  async start(): Promise<void> {
    if (this.started) return;
    this.started = true;
    window.addEventListener("message", this.receive);
  }

  async send(message: JSONRPCMessage): Promise<void> {
    this.eventTarget.postMessage(message, "*");
  }

  async close(): Promise<void> {
    if (!this.started) return;
    this.started = false;
    window.removeEventListener("message", this.receive);
    this.onclose?.();
  }
}

/**
 * MCP Apps 2026-01-26 host bridge.
 *
 * Core MCP traffic is deliberately left to the surrounding policy transports.
 * This bridge owns only the Apps handshake, iframe lifecycle, and the admitted
 * host operations used by Veoveo views.
 */
export class AppBridge {
  oncalltool?: ToolCallHandler;
  onreadresource?: ReadResourceHandler;
  onopenlink?: OpenLinkHandler;
  onrequestdisplaymode?: DisplayModeHandler;

  private transport?: Transport;
  private readonly sizeListeners = new Set<(event: { height?: number; width?: number }) => void>();

  constructor(
    _client: null,
    private readonly hostInfo: { name: string; version: string },
    private readonly hostCapabilities: Record<string, unknown>,
    private readonly options: AppBridgeOptions,
  ) {}

  addEventListener(
    event: "sizechange",
    listener: (event: { height?: number; width?: number }) => void,
  ): void {
    if (event === "sizechange") this.sizeListeners.add(listener);
  }

  async connect(transport: Transport): Promise<void> {
    if (this.transport) throw new Error("MCP App bridge is already connected");
    this.transport = transport;
    transport.onmessage = (message) => {
      void this.receive(message).catch((error: unknown) =>
        transport.onerror?.(error instanceof Error ? error : new Error(String(error))),
      );
    };
    await transport.start();
  }

  async close(): Promise<void> {
    const transport = this.transport;
    this.transport = undefined;
    if (transport) await transport.close();
  }

  async sendToolResult(result: CallToolResult): Promise<void> {
    await this.notify("ui/notifications/tool-result", result as Record<string, unknown>);
  }

  async sendToolInput(params: { arguments: Record<string, unknown> }): Promise<void> {
    await this.notify("ui/notifications/tool-input", params);
  }

  private async notify(method: string, params: Record<string, unknown>): Promise<void> {
    if (!this.transport) throw new Error("MCP App bridge is not connected");
    await this.transport.send({ jsonrpc: "2.0", method, params } as JSONRPCMessage);
  }

  private async reply(id: string | number, result: Record<string, unknown>): Promise<void> {
    await this.transport?.send({ jsonrpc: "2.0", id, result } as JSONRPCMessage);
  }

  private async reject(id: string | number, code: number, message: string): Promise<void> {
    await this.transport?.send({ jsonrpc: "2.0", id, error: { code, message } } as JSONRPCMessage);
  }

  private async receive(message: JSONRPCMessage): Promise<void> {
    const received: unknown = message;
    if (!isRecord(received) || typeof received.method !== "string") return;
    if (!("id" in received)) {
      if (received.method === "ui/notifications/size-changed" && isRecord(received.params)) {
        const height = typeof received.params.height === "number" ? received.params.height : undefined;
        const width = typeof received.params.width === "number" ? received.params.width : undefined;
        for (const listener of this.sizeListeners) listener({ height, width });
      }
      return;
    }
    if (!isJsonRpcRequest(received)) return;
    const request = received;

    try {
      switch (request.method) {
        case "ui/initialize":
          await this.reply(request.id, {
            protocolVersion: APPS_PROTOCOL_VERSION,
            hostInfo: this.hostInfo,
            hostCapabilities: this.hostCapabilities,
            hostContext: this.options.hostContext,
          });
          return;
        case "ui/request-display-mode":
          await this.reply(request.id, this.onrequestdisplaymode
            ? await this.onrequestdisplaymode()
            : { mode: "inline" });
          return;
        case "ui/open-link": {
          const url = request.params?.url;
          if (typeof url !== "string") {
            await this.reject(request.id, INVALID_PARAMS, "url must be a string");
            return;
          }
          if (!this.onopenlink) throw new Error("open-link handler is unavailable");
          await this.reply(request.id, await this.onopenlink({ url }));
          return;
        }
        case "tools/call": {
          const name = request.params?.name;
          const toolArguments = request.params?.arguments;
          if (
            typeof name !== "string" ||
            (toolArguments !== undefined && !isRecord(toolArguments))
          ) {
            await this.reject(request.id, INVALID_PARAMS, "invalid tools/call parameters");
            return;
          }
          if (!this.oncalltool) throw new Error("tool-call handler is unavailable");
          const inputResponses = request.params?.inputResponses;
          const requestState = request.params?.requestState;
          if (inputResponses !== undefined && !isRecord(inputResponses)) {
            await this.reject(request.id, INVALID_PARAMS, "inputResponses must be an object");
            return;
          }
          if (requestState !== undefined && typeof requestState !== "string") {
            await this.reject(request.id, INVALID_PARAMS, "requestState must be a string");
            return;
          }
          await this.reply(request.id, await this.oncalltool({
            name,
            arguments: toolArguments,
            inputResponses,
            requestState,
          }) as Record<string, unknown>);
          return;
        }
        case "resources/read": {
          const uri = request.params?.uri;
          if (typeof uri !== "string") {
            await this.reject(request.id, INVALID_PARAMS, "uri must be a string");
            return;
          }
          if (!this.onreadresource) throw new Error("resource-read handler is unavailable");
          await this.reply(request.id, await this.onreadresource({ uri }) as Record<string, unknown>);
          return;
        }
        default:
          await this.reject(request.id, -32601, `unsupported MCP App method ${request.method}`);
      }
    } catch (error) {
      await this.reject(
        request.id,
        INTERNAL_ERROR,
        error instanceof Error ? error.message : String(error),
      );
    }
  }
}
