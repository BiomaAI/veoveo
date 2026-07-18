import { useEffect, useRef } from "react";
import { appFrameUrl } from "../api";
import { attachAppBridge, type AppBridge } from "./bridge";
import type { AppDescriptor } from "../types";
import { useTheme } from "../theme";

/**
 * One sandboxed MCP App view. `sandbox="allow-scripts"` without
 * `allow-same-origin` gives the document an opaque origin: no cookies, no
 * storage, no same-origin fetch — its only capability is the postMessage
 * bridge, whose tool calls go through the BFF's same-server allowlist.
 */
export function AppFrame({ app }: { app: AppDescriptor }) {
  const { appTheme } = useTheme();
  const frameRef = useRef<HTMLIFrameElement>(null);
  const bridgeRef = useRef<AppBridge>(null);

  useEffect(() => {
    const iframe = frameRef.current;
    if (!iframe) return;
    const bridge = attachAppBridge(iframe, app, appTheme);
    bridgeRef.current = bridge;
    return () => {
      bridgeRef.current = null;
      bridge.dispose();
    };
  }, [app, appTheme]);

  return (
    <iframe
      ref={frameRef}
      className="app-frame"
      src={appFrameUrl(app.resourceUri)}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
      title={app.title ?? app.name}
    />
  );
}
