import { INTERNAL_ERROR, isJsonRpcRequest, type Transport } from "./protocol.ts";
import type { AppDescriptor } from "../types.ts";
import { openResourceEventStream, type ResourceEventStream } from "./resourceEventStream.ts";

export interface AppResourceHost {
  eventsUrl: string;
  open: (server: string, appUri: string, subscriptions: Array<{ subscriptionId: string; uri: string }>, signal?: AbortSignal | null) => Promise<Response>;
  unsubscribe: (subscriptionId: string) => Promise<unknown>;
}

interface ResourceListener {
  requestId: string | number;
  registrations: Array<{ subscriptionId: string; uri: string }>;
  source: ResourceEventStream;
}

function resourceOwnedByApp(app: AppDescriptor, uri: string): boolean {
  if (uri.includes("..")) return false;
  if (uri.startsWith(`${app.server}://`) && uri.length > app.server.length + 3) return true;
  return app.resourceDependencies.some((dependency) =>
    dependency.operations.includes("subscribe") &&
    uri.startsWith(dependency.uri_prefix) &&
    uri.startsWith(`${dependency.scheme}://`),
  );
}

/**
 * MCP Apps and core MCP remain separate protocols on the iframe channel.
 * Each final-profile `subscriptions/listen` request owns one authenticated
 * wake stream. Notifications carry no domain payload; views read current
 * state through their ordinary app-scoped `resources/read` permission.
 */
export function interceptResourceSubscriptions(
  inner: Transport,
  app: AppDescriptor,
  host: AppResourceHost,
): { transport: Transport; dispose: () => void } {
  const listeners = new Map<string, ResourceListener>();
  let disposed = false;
  const transport: Transport = {
    start: () => inner.start(),
    send: (message, options) => inner.send(message, options),
    close: () => inner.close(),
  };

  const reject = (id: string | number, message: string) =>
    inner.send({
      jsonrpc: "2.0",
      id,
      error: { code: INTERNAL_ERROR, message },
    });
  const report = (error: unknown) =>
    transport.onerror?.(error instanceof Error ? error : new Error(String(error)));
  const listenerKey = (id: string | number) => `${typeof id}:${id}`;
  const notify = (method: string, params: Record<string, unknown>) =>
    inner.send({
      jsonrpc: "2.0",
      method,
      params,
    } as never);
  const closeListener = (listener: ResourceListener) => {
    listener.source.close();
    listeners.delete(listenerKey(listener.requestId));
    return Promise.all(
      listener.registrations.map(({ subscriptionId }) =>
        host.unsubscribe(subscriptionId),
      ),
    );
  };
  const open = (requestId: string | number, uris: string[]) => {
    const registrations = uris.map((uri) => ({
      subscriptionId: crypto.randomUUID(),
      uri,
    }));
    const key = listenerKey(requestId);
    const source = openResourceEventStream(
      host.eventsUrl,
      {
        onOpen: () => {
          if (disposed || !listeners.has(key)) return;
          void notify("notifications/subscriptions/acknowledged", {
            notifications: { resourceSubscriptions: uris },
            _meta: { "io.modelcontextprotocol/subscriptionId": requestId },
          }).catch(report);
        },
        onEvent: (event) => {
          if (event.type !== "resource-updated" || !listeners.has(key)) return;
          try {
            const params = JSON.parse(event.data) as { uri?: string; uris?: string[] };
            if (typeof params.uri === "string" && uris.includes(params.uri)) {
              void notify("notifications/resources/updated", { uri: params.uri }).catch(report);
            }
            if (Array.isArray(params.uris)) {
              for (const uri of params.uris) {
                if (uris.includes(uri)) {
                  void notify("notifications/resources/updated", { uri }).catch(report);
                }
              }
            }
          } catch (error) {
            report(error);
          }
        },
        onInitialError: (error) => {
          listeners.delete(key);
          void reject(requestId, "subscription stream failed to open").catch(report);
          report(error);
        },
      },
      (_input, init) =>
        host.open(app.server, app.resourceUri, registrations, init?.signal),
    );
    listeners.set(key, { requestId, registrations, source });
  };

  inner.onclose = () => transport.onclose?.();
  inner.onerror = (error) => transport.onerror?.(error);
  inner.onmessage = (message, extra) => {
    if (
      !isJsonRpcRequest(message) ||
      message.method !== "subscriptions/listen"
    ) {
      transport.onmessage?.(message, extra);
      return;
    }
    const notifications = message.params?.notifications;
    const uris =
      typeof notifications === "object" && notifications !== null && !Array.isArray(notifications)
        ? (notifications as { resourceSubscriptions?: unknown }).resourceSubscriptions
        : undefined;
    if (
      !Array.isArray(uris) ||
      uris.length === 0 ||
      uris.length > 64 ||
      !uris.every((uri): uri is string => typeof uri === "string" && resourceOwnedByApp(app, uri)) ||
      new Set(uris).size !== uris.length
    ) {
      void reject(message.id, "listen request must contain unique app-owned resource subscriptions");
      return;
    }
    const key = listenerKey(message.id);
    if (listeners.has(key)) {
      void reject(message.id, "listen request id is already active");
      return;
    }
    open(message.id, uris);
  };

  return {
    transport,
    dispose: () => {
      disposed = true;
      for (const listener of [...listeners.values()]) {
        void closeListener(listener).catch((error: unknown) =>
          console.error("MCP App subscription close failed", error)
        );
      }
    },
  };
}

