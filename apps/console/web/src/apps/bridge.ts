/**
 * Host side of the MCP Apps (ext-apps 2026-01-26) postMessage protocol.
 *
 * The app view runs in an opaque-origin iframe (`sandbox="allow-scripts"`,
 * no `allow-same-origin`), so messages are matched by the frame's window
 * identity plus the `"null"` origin string, and posts target `"*"` — the
 * window handle itself is per-frame, so this cannot leak across frames.
 */
import { callAppTool } from "../api";
import type { AppDescriptor } from "../types";

interface JsonRpcMessage {
  jsonrpc?: string;
  id?: number | string;
  method?: string;
  params?: Record<string, unknown>;
  result?: unknown;
  error?: { code: number; message: string };
}

export interface AppBridge {
  dispose: () => void;
  notifyToolResult: (result: unknown) => void;
  notifyToolInput: (args: Record<string, unknown>) => void;
}

const MAX_FRAME_HEIGHT = 1400;
const MIN_FRAME_HEIGHT = 180;

export function attachAppBridge(iframe: HTMLIFrameElement, app: AppDescriptor): AppBridge {
  const post = (message: JsonRpcMessage) => {
    iframe.contentWindow?.postMessage({ jsonrpc: "2.0", ...message }, "*");
  };

  const respond = (id: number | string, result: unknown) => post({ id, result });
  const respondError = (id: number | string, code: number, message: string) =>
    post({ id, error: { code, message } });

  const onMessage = (event: MessageEvent) => {
    if (event.source !== iframe.contentWindow || event.origin !== "null") return;
    const message = event.data as JsonRpcMessage;
    if (!message || message.jsonrpc !== "2.0" || !message.method) return;
    const { id, method, params } = message;

    switch (method) {
      case "ui/initialize": {
        if (id === undefined) return;
        respond(id, {
          hostInfo: { name: "veoveo-console", version: "1.0.0" },
          hostCapabilities: { toolCalling: true, openLinks: true },
          hostContext: {
            theme: "light",
            displayMode: "inline",
            locale: navigator.language,
            containerDimensions: { width: iframe.clientWidth },
          },
        });
        return;
      }
      case "ui/notifications/initialized":
        return;
      case "tools/call": {
        if (id === undefined) return;
        const name = typeof params?.name === "string" ? params.name : "";
        const args = (params?.arguments ?? {}) as Record<string, unknown>;
        if (!app.tools.some((tool) => tool.name === name)) {
          respondError(id, -32602, `tool ${name} is not available to this app`);
          return;
        }
        callAppTool(app.server, app.resourceUri, name, args)
          .then((result) => respond(id, result))
          .catch((cause: unknown) => {
            respondError(id, -32000, cause instanceof Error ? cause.message : "tool call failed");
          });
        return;
      }
      case "ui/notifications/size-changed": {
        const height = Number(params?.height);
        if (Number.isFinite(height)) {
          iframe.style.height = `${Math.min(MAX_FRAME_HEIGHT, Math.max(MIN_FRAME_HEIGHT, height))}px`;
        }
        return;
      }
      case "ui/open-link": {
        const url = typeof params?.url === "string" ? params.url : "";
        const confirmed =
          url.startsWith("https://") &&
          window.confirm(`This app wants to open:\n${url}\n\nOpen in a new tab?`);
        if (confirmed) window.open(url, "_blank", "noopener,noreferrer");
        if (id !== undefined) respond(id, { opened: confirmed });
        return;
      }
      case "ui/request-display-mode": {
        if (id !== undefined) respond(id, { displayMode: "inline" });
        return;
      }
      default: {
        if (id !== undefined) respondError(id, -32601, `method ${method} is not supported`);
        return;
      }
    }
  };

  window.addEventListener("message", onMessage);

  return {
    dispose: () => {
      post({ method: "ui/resource-teardown", params: {} });
      window.removeEventListener("message", onMessage);
    },
    notifyToolResult: (result) => {
      post({ method: "ui/notifications/tool-result", params: { result } });
    },
    notifyToolInput: (args) => {
      post({ method: "ui/notifications/tool-input", params: { arguments: args } });
    },
  };
}
