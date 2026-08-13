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
import {
  callAppTool,
  cancelAppTask,
  getAppTask,
  getAppTaskResult,
  openAppResourceEvents,
  readAppResource,
  sendAgentMessage,
  unsubscribeAppResource,
} from "../api";
import { appFrameOuterHeight } from "../appFrameSizing";
import { isFullBleedApp } from "../appPresentation";
import type { AppDescriptor } from "../types";
import type { AppTheme } from "../theme";
import { AGENT_MESSAGE_METHOD, appAgentMessageRequest } from "./agentMessage";
import {
  openResourceEventStream,
  type ResourceEventStream,
} from "./resourceEventStream";
import { interceptResourceReadRequests } from "./resourceRead";

export interface AppBridge {
  dispose: () => void;
  notifyToolResult: (result: CallToolResult) => void;
  notifyToolInput: (args: Record<string, unknown>) => void;
}

export type InternalAppLinkHandler = (url: string) => boolean;

const TASK_METHODS = new Set(["tasks/get", "tasks/result", "tasks/cancel"]);

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
    if (!isJSONRPCRequest(message) || message.method !== AGENT_MESSAGE_METHOD) {
      transport.onmessage?.(message, extra);
      return;
    }
    const request = appAgentMessageRequest(app, message.params);
    if (request === undefined) {
      void inner.send({
        jsonrpc: "2.0",
        id: message.id,
        error: { code: ErrorCode.InvalidParams, message: "agent message request is not allowed" },
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
              code: ErrorCode.InternalError,
              message: error instanceof Error ? error.message : String(error),
            },
          }),
      )
      .catch((error: unknown) => console.error("MCP App agent-message reply failed", error));
  };
  return transport;
}

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

interface ResourceSubscription {
  id: string;
  opened: boolean;
  requests: Array<string | number>;
}

function resourceOwnedByApp(app: AppDescriptor, uri: string): boolean {
  return !uri.includes("..") && uri.startsWith(`${app.server}://`) && uri.length > app.server.length + 3;
}

/**
 * MCP Apps 2026-01-26 does not carry MCP resource subscribe/unsubscribe or
 * resource-updated notifications. Veoveo's generic host adapter projects
 * those three frames through one authenticated SSE wake stream while every
 * payload read remains an ordinary app-scoped `resources/read`.
 */
function interceptResourceSubscriptions(
  inner: Transport,
  app: AppDescriptor
): { transport: Transport; dispose: () => void } {
  const subscriptions = new Map<string, ResourceSubscription>();
  let source: ResourceEventStream | undefined;
  let openScheduled = false;
  let recoveryTimer: ReturnType<typeof setTimeout> | undefined;
  let generation = 0;
  let disposed = false;
  const transport: Transport = {
    start: () => inner.start(),
    send: (message, options) => inner.send(message, options),
    close: () => inner.close(),
  };

  const reply = (id: string | number, result: Result) =>
    inner.send({ jsonrpc: "2.0", id, result });
  const reject = (id: string | number, message: string) =>
    inner.send({
      jsonrpc: "2.0",
      id,
      error: { code: ErrorCode.InternalError, message },
    });
  const report = (error: unknown) =>
    transport.onerror?.(error instanceof Error ? error : new Error(String(error)));
  const notifyUpdated = (uri: string) => {
    if (!subscriptions.has(uri)) return;
    void inner.send({
      jsonrpc: "2.0",
      method: "ui/notifications/resource-updated",
      params: { uri },
    } as never).catch(report);
  };
  const open = () => {
    openScheduled = false;
    source?.close();
    source = undefined;
    const active = [...subscriptions.entries()];
    if (disposed || active.length === 0) return;
    const activeGeneration = ++generation;
    const batch = active.map(([uri, subscription]) => ({
      subscriptionId: subscription.id,
      uri,
    }));
    source = openResourceEventStream(
      "/console/api/apps/resource-events",
      {
        onOpen: () => {
          if (disposed || generation !== activeGeneration) return;
          for (const [uri, subscription] of active) {
            if (subscriptions.get(uri) !== subscription) continue;
            if (subscription.opened) {
              notifyUpdated(uri);
              continue;
            }
            subscription.opened = true;
            for (const requestId of subscription.requests) {
              void reply(requestId, {}).catch(report);
            }
            subscription.requests = [];
          }
        },
        onEvent: (event) => {
          if (event.type !== "resource-updated" || generation !== activeGeneration) return;
          try {
            const params = JSON.parse(event.data) as { uri?: string; uris?: string[] };
            if (typeof params.uri === "string") notifyUpdated(params.uri);
            if (Array.isArray(params.uris)) {
              for (const uri of params.uris) notifyUpdated(uri);
            }
          } catch (error) {
            report(error);
          }
        },
        onInitialError: (error) => {
          if (disposed || generation !== activeGeneration) return;
          for (const [uri, subscription] of active) {
            if (subscription.opened || subscriptions.get(uri) !== subscription) continue;
            subscriptions.delete(uri);
            for (const requestId of subscription.requests) {
              void reject(requestId, "resource subscription stream failed to open").catch(report);
            }
          }
          if (subscriptions.size > 0) {
            recoveryTimer = setTimeout(() => {
              recoveryTimer = undefined;
              open();
            }, 250);
          }
          report(error);
        },
      },
      (_input, init) => openAppResourceEvents(app.server, app.resourceUri, batch, init?.signal),
    );
  };
  const scheduleOpen = () => {
    if (disposed || openScheduled) return;
    if (recoveryTimer !== undefined) {
      clearTimeout(recoveryTimer);
      recoveryTimer = undefined;
    }
    openScheduled = true;
    queueMicrotask(open);
  };
  const close = (uri: string) => {
    const subscription = subscriptions.get(uri);
    if (!subscription) return Promise.resolve();
    subscriptions.delete(uri);
    for (const requestId of subscription.requests) {
      void reject(requestId, "resource subscription was cancelled before admission").catch(report);
    }
    scheduleOpen();
    return unsubscribeAppResource(subscription.id);
  };

  inner.onclose = () => transport.onclose?.();
  inner.onerror = (error) => transport.onerror?.(error);
  inner.onmessage = (message, extra) => {
    if (
      !isJSONRPCRequest(message) ||
      (message.method !== "resources/subscribe" && message.method !== "resources/unsubscribe")
    ) {
      transport.onmessage?.(message, extra);
      return;
    }
    const { uri } = (message.params ?? {}) as { uri?: unknown };
    if (typeof uri !== "string" || !resourceOwnedByApp(app, uri)) {
      void reject(message.id, "resource subscription is not owned by this App's server");
      return;
    }
    if (message.method === "resources/unsubscribe") {
      void close(uri).then(
        () => reply(message.id, {}),
        (error: unknown) => reject(message.id, error instanceof Error ? error.message : String(error))
      );
      return;
    }
    if (subscriptions.has(uri)) {
      const subscription = subscriptions.get(uri)!;
      if (subscription.opened) void reply(message.id, {}).catch(report);
      else subscription.requests.push(message.id);
      return;
    }
    const subscriptionId = crypto.randomUUID();
    const subscription: ResourceSubscription = {
      id: subscriptionId,
      opened: false,
      requests: [message.id],
    };
    subscriptions.set(uri, subscription);
    scheduleOpen();
  };

  return {
    transport,
    dispose: () => {
      disposed = true;
      generation += 1;
      if (recoveryTimer !== undefined) clearTimeout(recoveryTimer);
      source?.close();
      for (const uri of [...subscriptions.keys()]) {
        void close(uri).catch((error: unknown) =>
          console.error("MCP App resource unsubscribe failed", error)
        );
      }
    },
  };
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

  const subscriptions = interceptResourceSubscriptions(
    interceptTaskRequests(
      interceptResourceReadRequests(
        interceptAgentMessages(
          new PostMessageTransport(iframe.contentWindow, iframe.contentWindow),
          app,
        ),
        app,
        readAppResource,
      ),
      app,
    ),
    app,
  );
  const transport = subscriptions.transport;
  transport.onerror = (error) => console.error("MCP App transport error", error);
  void bridge.connect(transport).catch((error: unknown) => {
    console.error("MCP App bridge failed", error);
  });

  return {
    dispose: () => {
      subscriptions.dispose();
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
