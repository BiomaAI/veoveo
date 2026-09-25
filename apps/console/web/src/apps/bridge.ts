/**
 * Host side of the stable MCP Apps protocol.
 *
 * Veoveo's AppBridge owns JSON-RPC validation, protocol negotiation, and
 * lifecycle handling. Veoveo supplies the product policy around that bridge:
 * app-scoped tool allowlisting, confirmed HTTPS links, inline display, and
 * bounded frame sizing.
 */
import { AppBridge as McpAppBridge, PostMessageTransport } from "./appBridge.ts";
import {
  INTERNAL_ERROR,
  INVALID_PARAMS,
  isJsonRpcRequest,
  type CallToolResult,
  type InputResponses,
  type JsonRpcRequest,
  type Result,
  type Transport,
} from "./protocol.ts";
import {
  callAppTool,
  cancelAppTask,
  getAppTask,
  loadRecordingProjectionStream,
  openAppResourceEvents,
  readAppResource,
  sendAgentMessage,
  unsubscribeAppResource,
  updateAppTask,
} from "../api";
import { appFrameOuterHeight } from "../appFrameSizing";
import { isFullBleedApp } from "../appPresentation";
import type { AppDescriptor } from "../types";
import type { AppTheme } from "../theme";
import { AGENT_MESSAGE_METHOD, appAgentMessageRequest } from "./agentMessage";
import { interceptResourceSubscriptions } from "./resourceTransport.ts";
import { interceptResourceReadRequests } from "./resourceRead.ts";
import { interceptRecordingProjectionStreams } from "./recordingProjectionStream.ts";

export interface AppBridge {
  dispose: () => void;
  setTheme: (theme: AppTheme) => void;
  notifyToolResult: (result: CallToolResult) => void;
  notifyToolInput: (args: Record<string, unknown>) => void;
}

export type InternalAppLinkHandler = (url: string) => boolean;

const TASK_METHODS = new Set(["tasks/get", "tasks/update", "tasks/cancel"]);
/**
 * Agent messaging is admitted only for exact targets declared by the App
 * resource. The BFF retains the authenticated human, CSRF, Work Context,
 * policy, audit, idempotency, and durable-wake boundaries.
 */
function interceptAgentMessages(inner: Transport, app: AppDescriptor): Transport {
  const transport: Transport = {
    start: () => inner.start(),
    send: (message, options) => inner.send(message, options),
    close: () => inner.close(),
  };
  inner.onclose = () => transport.onclose?.();
  inner.onerror = (error) => transport.onerror?.(error);
  inner.onmessage = (message, extra) => {
    if (!isJsonRpcRequest(message) || message.method !== AGENT_MESSAGE_METHOD) {
      transport.onmessage?.(message, extra);
      return;
    }
    const request = appAgentMessageRequest(app, message.params);
    if (request === undefined) {
      void inner.send({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: INVALID_PARAMS, message: "agent message request is not allowed" },
      });
      return;
    }
    sendAgentMessage(request.agentId, request.requestId, request.message)
      .then(
        (result) =>
          inner.send({ jsonrpc: "2.0", id: message.id, result: result as unknown as Result }),
        (error: unknown) =>
          inner.send({
            jsonrpc: "2.0",
            id: message.id,
            error: {
              code: INTERNAL_ERROR,
              message: error instanceof Error ? error.message : String(error),
            },
          }),
      )
      .catch((error: unknown) => console.error("MCP App agent-message reply failed", error));
  };
  return transport;
}

async function dispatchTaskRequest(app: AppDescriptor, request: JsonRpcRequest): Promise<Result> {
  const { taskId } = (request.params ?? {}) as { taskId?: unknown };
  if (typeof taskId !== "string" || taskId.length === 0) {
    throw new Error("taskId must be a non-empty string");
  }
  switch (request.method) {
    case "tasks/get":
      return getAppTask(app.server, app.resourceUri, taskId);
    case "tasks/update": {
      const { inputResponses } = request.params as { inputResponses?: unknown };
      if (typeof inputResponses !== "object" || inputResponses === null || Array.isArray(inputResponses)) {
        throw new Error("inputResponses must be an object");
      }
      return updateAppTask(
        app.server,
        app.resourceUri,
        taskId,
        inputResponses as InputResponses,
      );
    }
    case "tasks/cancel":
      return cancelAppTask(app.server, app.resourceUri, taskId);
    default:
      throw new Error(`unsupported task method ${request.method}`);
  }
}

/**
 * Tasks are a core MCP extension rather than part of MCP Apps. Task lifecycle
 * requests cross the Apps transport seam and are admitted only for task IDs
 * created by this app view.
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
      !isJsonRpcRequest(message) ||
      !TASK_METHODS.has(message.method)
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
              code: INTERNAL_ERROR,
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
  theme: AppTheme,
  openInternalLink?: InternalAppLinkHandler,
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
        _meta: {
          "io.veoveo/agent-message-targets": app.agentMessageTargets,
        },
      },
    },
  );

  bridge.oncalltool = async ({
    name,
    arguments: toolArguments,
    inputResponses,
    requestState,
  }) => {
    if (!app.tools.some((tool) => tool.name === name)) {
      throw new Error(`tool ${name} is not available to this app`);
    }
    return callAppTool(app.server, app.resourceUri, name, toolArguments ?? {}, {
      inputResponses,
      requestState,
    });
  };
  bridge.onreadresource = async ({ uri }) => {
    if (!uri.startsWith(`${app.server}://`) && !uri.startsWith(`ui://${app.server}/`)) {
      throw new Error(`resource ${uri} is not owned by this app's server`);
    }
    return readAppResource(app.server, app.resourceUri, uri);
  };
  bridge.onopenlink = async ({ url }) => {
    if (url.startsWith("ui://") || url.startsWith("veoveo-console://")) {
      return openInternalLink?.(url) ? {} : { isError: true };
    }
    const confirmed =
      url.startsWith("https://") &&
      window.confirm(`This app wants to open:\n${url}\n\nOpen in a new tab?`);
    if (!confirmed) return { isError: true };
    window.open(url, "_blank", "noopener,noreferrer");
    return {};
  };
  bridge.onrequestdisplaymode = async () => ({ mode: "inline" });
  bridge.addEventListener("sizechange", ({ height }) => {
    if (isFullBleedApp(app)) return;
    if (height === undefined || !Number.isFinite(height)) return;
    const nonContentHeight = Math.max(0, iframe.offsetHeight - iframe.clientHeight);
    iframe.style.height = `${appFrameOuterHeight(height, nonContentHeight)}px`;
  });

  const projectionStreams = interceptRecordingProjectionStreams(
    new PostMessageTransport(iframe.contentWindow, iframe.contentWindow),
    app,
    (message, transfer) => iframe.contentWindow?.postMessage(message, "*", transfer),
    loadRecordingProjectionStream,
  );
  const subscriptions = interceptResourceSubscriptions(
    interceptTaskRequests(
      interceptResourceReadRequests(
        interceptAgentMessages(
          projectionStreams.transport,
          app,
        ),
        app,
        readAppResource,
      ),
      app,
    ),
    app,
    { eventsUrl: "/console/api/apps/resource-events", open: openAppResourceEvents, unsubscribe: unsubscribeAppResource },
  );
  const transport = subscriptions.transport;
  transport.onerror = (error) => console.error("MCP App transport error", error);
  void bridge.connect(transport).catch((error: unknown) => {
    console.error("MCP App bridge failed", error);
  });

  return {
    setTheme: (theme) => {
      void bridge.setTheme(theme).catch((error: unknown) => console.error("MCP App theme update failed", error));
    },
    dispose: () => {
      subscriptions.dispose();
      projectionStreams.dispose();
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
