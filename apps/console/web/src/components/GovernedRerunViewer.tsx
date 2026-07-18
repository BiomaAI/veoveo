import { useEffect, useRef, useState } from "react";
import { WebViewer } from "@rerun-io/web-viewer";

export interface GovernedRerunSource {
  mode: "live" | "replay";
  urls: string[];
}

type ViewerStatus =
  | { state: "loading"; delayed: boolean }
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
  const [status, setStatus] = useState<ViewerStatus>({
    state: "loading",
    delayed: false,
  });

  useEffect(() => {
    const viewer = new WebViewer();
    let active = true;
    let removeOpenListener: (() => void) | undefined;
    let delayedNotice: number | undefined;
    void viewer
      .start(null, host.current, {
        width: "100%",
        height: "100%",
        hide_welcome_screen: true,
        allow_fullscreen: true,
      })
      .then(() => {
        if (!active) return;
        delayedNotice = window.setTimeout(() => {
          if (active) {
            setStatus({
              state: "loading",
              delayed: true,
            });
          }
        }, 20_000);
        removeOpenListener = viewer.once("recording_open", () => {
          if (!active) return;
          if (delayedNotice !== undefined) window.clearTimeout(delayedNotice);
          setStatus({ state: "open" });
        });
        viewer.open(source.urls, { follow_if_http: source.mode === "live" });
      })
      .catch((cause: unknown) => {
        if (!active) return;
        if (delayedNotice !== undefined) window.clearTimeout(delayedNotice);
        const message = cause instanceof Error ? cause.message : "Rerun playback failed";
        console.error("Governed Rerun source failed", cause);
        setStatus({ state: "error", message });
      });

    return () => {
      active = false;
      if (delayedNotice !== undefined) window.clearTimeout(delayedNotice);
      removeOpenListener?.();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
    };
  }, [recordingId, source]);

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
          <strong>
            {status.delayed
              ? "The recording is still loading"
              : source.mode === "live"
                ? "Connecting to live capture"
                : "Preparing replay"}
          </strong>
          <span>
            {status.delayed
              ? "Authorized data is still streaming into Rerun. Playback will open automatically; large recordings can take longer."
              : source.mode === "live"
              ? "Loading captured history and following the active governed RRD segment."
              : "Normalizing authorized segments into one Rerun timeline."}
          </span>
        </div>
      ) : null}
    </div>
  );
}
