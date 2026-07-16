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
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { callAppTool } from "../api";
import type { AppDescriptor } from "../types";

export interface AppBridge {
  dispose: () => void;
  notifyToolResult: (result: CallToolResult) => void;
  notifyToolInput: (args: Record<string, unknown>) => void;
}

const MAX_FRAME_HEIGHT = 1400;
const MIN_FRAME_HEIGHT = 180;

export function attachAppBridge(iframe: HTMLIFrameElement, app: AppDescriptor): AppBridge {
  if (!iframe.contentWindow) throw new Error("MCP App frame is not ready");

  const bridge = new McpAppBridge(
    null,
    { name: "veoveo-console", version: "0.1.0" },
    { openLinks: {}, serverTools: {} },
    {
      hostContext: {
        theme: "light",
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

  const transport = new PostMessageTransport(iframe.contentWindow, iframe.contentWindow);
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
