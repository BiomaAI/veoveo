/**
 * Host side of the stable MCP Apps protocol.
 *
 * The official AppBridge owns JSON-RPC validation, protocol negotiation, and
 * lifecycle handling. Veoveo supplies the product policy around that bridge:
 * app-scoped tool allowlisting, confirmed HTTPS links, inline display, and
 * bounded frame sizing.
 */
import {
  AppBridge as McpAppBridge,
  PostMessageTransport,
} from "@modelcontextprotocol/ext-apps/app-bridge";
import type { Transport } from "@modelcontextprotocol/sdk/shared/transport.js";
import {
  ErrorCode,
  isJSONRPCRequest,
  type CallToolResult,
  type JSONRPCRequest,
  type Result,
  type TaskMetadata,
} from "@modelcontextprotocol/sdk/types.js";
import { callAppTool, cancelAppTask, getAppTask, getAppTaskResult, readAppResource } from "../api";
import type { AppDescriptor } from "../types";
import type { AppTheme } from "../theme";

export interface AppBridge {
  dispose: () => void;
  notifyToolResult: (result: CallToolResult) => void;
  notifyToolInput: (args: Record<string, unknown>) => void;
}

const MAX_FRAME_HEIGHT = 1400;
const MIN_FRAME_HEIGHT = 180;

const TASK_METHODS = new Set(["tasks/get", "tasks/result", "tasks/cancel"]);

function isTaskAugmentedCall(request: JSONRPCRequest): boolean {
  if (request.method !== "tools/call") return false;
  const { task } = (request.params ?? {}) as { task?: unknown };
  return typeof task === "object" && task !== null;
}

async function dispatchTaskRequest(app: AppDescriptor, request: JSONRPCRequest): Promise<Result> {
  if (request.method === "tools/call") {
    const { name, arguments: toolArguments, task } = request.params as {
      name?: unknown;
      arguments?: Record<string, unknown>;
      task?: TaskMetadata;
    };
    if (typeof name !== "string" || !app.tools.some((tool) => tool.name === name)) {
      throw new Error(`tool ${String(name)} is not available to this app`);
    }
    return callAppTool(app.server, app.resourceUri, name, toolArguments ?? {}, task ?? {});
  }
  const { taskId } = (request.params ?? {}) as { taskId?: unknown };
  if (typeof taskId !== "string" || taskId.length === 0) {
    throw new Error("taskId must be a non-empty string");
  }
  switch (request.method) {
    case "tasks/get":
      return getAppTask(app.server, app.resourceUri, taskId);
    case "tasks/result":
      return getAppTaskResult(app.server, app.resourceUri, taskId);
    case "tasks/cancel":
      return cancelAppTask(app.server, app.resourceUri, taskId);
    default:
      throw new Error(`unsupported task method ${request.method}`);
  }
}

/**
 * The stock AppBridge refuses the task lifecycle outright (its task
 * capability asserts throw), so task traffic is answered at the transport
 * seam before the bridge sees it: `tasks/get|result|cancel` and
 * task-augmented `tools/call` go straight to the BFF task proxies for the
 * app's own server; every other frame flows to the AppBridge unchanged.
 */
function interceptTaskRequests(inner: Transport, app: AppDescriptor): Transport {
  const transport: Transport = {
    start: () => inner.start(),
    send: (message, options) => inner.send(message, options),
    close: () => inner.close(),
  };
  inner.onclose = () => transport.onclose?.();
  inner.onerror = (error) => transport.onerror?.(error);
  inner.onmessage = (message, extra) => {
    if (
      !isJSONRPCRequest(message) ||
      !(TASK_METHODS.has(message.method) || isTaskAugmentedCall(message))
    ) {
      transport.onmessage?.(message, extra);
      return;
    }
    dispatchTaskRequest(app, message)
      .then(
        (result) => inner.send({ jsonrpc: "2.0", id: message.id, result }),
        (error: unknown) =>
          inner.send({
            jsonrpc: "2.0",
            id: message.id,
            error: {
              code: ErrorCode.InternalError,
              message: error instanceof Error ? error.message : String(error),
            },
          })
      )
      .catch((error: unknown) => console.error("MCP App task reply failed", error));
  };
  return transport;
}

export function attachAppBridge(
  iframe: HTMLIFrameElement,
  app: AppDescriptor,
  theme: AppTheme
): AppBridge {
  if (!iframe.contentWindow) throw new Error("MCP App frame is not ready");

  const bridge = new McpAppBridge(
    null,
    { name: "veoveo-console", version: "0.1.0" },
    { openLinks: {}, serverTools: {}, serverResources: {} },
    {
      hostContext: {
        theme,
        displayMode: "inline",
        availableDisplayModes: ["inline"],
        locale: navigator.language,
        platform: "web",
        containerDimensions: { width: iframe.clientWidth },
      },
    },
  );

  bridge.oncalltool = async ({ name, arguments: toolArguments }) => {
    if (!app.tools.some((tool) => tool.name === name)) {
      throw new Error(`tool ${name} is not available to this app`);
    }
    return callAppTool(app.server, app.resourceUri, name, toolArguments ?? {});
  };
  bridge.onreadresource = async ({ uri }) => {
    if (!uri.startsWith(`${app.server}://`) && !uri.startsWith(`ui://${app.server}/`)) {
      throw new Error(`resource ${uri} is not owned by this app's server`);
    }
    return readAppResource(app.server, app.resourceUri, uri);
  };
  bridge.onopenlink = async ({ url }) => {
    const confirmed =
      url.startsWith("https://") &&
      window.confirm(`This app wants to open:\n${url}\n\nOpen in a new tab?`);
    if (!confirmed) return { isError: true };
    window.open(url, "_blank", "noopener,noreferrer");
    return {};
  };
  bridge.onrequestdisplaymode = async () => ({ mode: "inline" });
  bridge.addEventListener("sizechange", ({ height }) => {
    if (height === undefined || !Number.isFinite(height)) return;
    iframe.style.height = `${Math.min(MAX_FRAME_HEIGHT, Math.max(MIN_FRAME_HEIGHT, height))}px`;
  });

  const transport = interceptTaskRequests(
    new PostMessageTransport(iframe.contentWindow, iframe.contentWindow),
    app
  );
  transport.onerror = (error) => console.error("MCP App transport error", error);
  void bridge.connect(transport).catch((error: unknown) => {
    console.error("MCP App bridge failed", error);
  });

  return {
    dispose: () => {
      void bridge.close();
    },
    notifyToolResult: (result) => {
      void bridge.sendToolResult(result);
    },
    notifyToolInput: (args) => {
      void bridge.sendToolInput({ arguments: args });
    },
  };
}
