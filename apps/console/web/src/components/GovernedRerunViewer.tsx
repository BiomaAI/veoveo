import { useEffect, useRef, useState } from "react";
import { WebViewer } from "@rerun-io/web-viewer";

export interface GovernedRerunSource {
  mode: "live" | "replay";
  url: string;
}

type ViewerStatus =
  | { state: "loading" }
  | { state: "open" }
  | { state: "error"; message: string };

export default function GovernedRerunViewer({
  recordingId,
  source,
}: {
  recordingId: string;
  source: GovernedRerunSource;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<ViewerStatus>({ state: "loading" });

  useEffect(() => {
    const viewer = new WebViewer();
    let active = true;
    let removeOpenListener: (() => void) | undefined;
    const openTimeout = window.setTimeout(() => {
      if (active) {
        setStatus({
          state: "error",
          message: "The governed RRD source did not open within 20 seconds.",
        });
      }
    }, 20_000);
    void viewer
      .start(null, host.current, {
        width: "100%",
        height: "100%",
        hide_welcome_screen: true,
        allow_fullscreen: true,
      })
      .then(() => {
        if (!active) return;
        removeOpenListener = viewer.once("recording_open", () => {
          if (!active) return;
          window.clearTimeout(openTimeout);
          setStatus({ state: "open" });
        });
        viewer.open(source.url, { follow_if_http: source.mode === "live" });
      })
      .catch((cause: unknown) => {
        if (!active) return;
        window.clearTimeout(openTimeout);
        const message = cause instanceof Error ? cause.message : "Rerun playback failed";
        console.error("Governed Rerun source failed", cause);
        setStatus({ state: "error", message });
      });

    return () => {
      active = false;
      window.clearTimeout(openTimeout);
      removeOpenListener?.();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
    };
  }, [recordingId, source.mode, source.url]);

  return (
    <div className="rerun-web-viewer">
      <div ref={host} className="rerun-web-viewer-host" />
      {status.state === "error" ? (
        <div className="recording-viewer-state recording-viewer-overlay recording-viewer-error">
          <strong>Rerun could not open this recording.</strong>
          <span>{status.message}</span>
        </div>
      ) : status.state === "loading" ? (
        <div className="recording-viewer-state recording-viewer-overlay">
          <div className="loading-mark" />
          <strong>{source.mode === "live" ? "Connecting to live capture" : "Preparing replay"}</strong>
          <span>
            {source.mode === "live"
              ? "Following the active governed RRD segment."
              : "Normalizing authorized segments into one Rerun timeline."}
          </span>
        </div>
      ) : null}
    </div>
  );
}
