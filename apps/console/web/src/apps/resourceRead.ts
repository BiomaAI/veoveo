import type { Transport } from "@modelcontextprotocol/sdk/shared/transport.js";
import {
  ErrorCode,
  isJSONRPCRequest,
  type ReadResourceResult,
  type Result,
} from "@modelcontextprotocol/sdk/types.js";
import type { AppDescriptor } from "../types";

export type AppResourceReader = (
  server: string,
  appUri: string,
  uri: string,
) => Promise<ReadResourceResult>;

function ownedResource(app: AppDescriptor, uri: string): boolean {
  return (
    !uri.includes("..") &&
    (uri.startsWith(`${app.server}://`) || uri.startsWith(`ui://${app.server}/`))
  );
}

function declaredDependency(app: AppDescriptor, uri: string): boolean {
  if (uri.includes("..")) return false;
  return app.resourceDependencies.some(
    (dependency) =>
      dependency.app_resource === app.resourceUri &&
      dependency.operations.includes("read") &&
      uri.startsWith(dependency.uri_prefix) &&
      uri.startsWith(`${dependency.scheme}://`),
  );
}

/**
 * Settle App resource reads at the transport seam. This retains the BFF and
 * Gateway authorization walls while ensuring every iframe request receives
 * one protocol response.
 */
export function interceptResourceReadRequests(
  inner: Transport,
  app: AppDescriptor,
  read: AppResourceReader,
): Transport {
  const transport: Transport = {
    start: () => inner.start(),
    send: (message, options) => inner.send(message, options),
    close: () => inner.close(),
  };

  const reply = (id: string | number, result: ReadResourceResult) =>
    inner.send({ jsonrpc: "2.0", id, result: result as Result });
  const reject = (id: string | number, code: ErrorCode, message: string) =>
    inner.send({ jsonrpc: "2.0", id, error: { code, message } });
  const report = (error: unknown) =>
    transport.onerror?.(error instanceof Error ? error : new Error(String(error)));

  inner.onclose = () => transport.onclose?.();
  inner.onerror = report;
  inner.onmessage = (message, extra) => {
    if (!isJSONRPCRequest(message) || message.method !== "resources/read") {
      transport.onmessage?.(message, extra);
      return;
    }
    const { uri } = (message.params ?? {}) as { uri?: unknown };
    if (
      typeof uri !== "string" ||
      (!ownedResource(app, uri) && !declaredDependency(app, uri))
    ) {
      void reject(message.id, ErrorCode.InvalidParams, "resource is not declared for this App")
        .catch(report);
      return;
    }
    read(app.server, app.resourceUri, uri)
      .then(
        (result) => reply(message.id, result),
        (error: unknown) =>
          reject(
            message.id,
            ErrorCode.InternalError,
            error instanceof Error ? error.message : String(error),
          ),
      )
      .catch(report);
  };

  return transport;
}
