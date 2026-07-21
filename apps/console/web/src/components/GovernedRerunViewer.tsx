import { useEffect, useRef, useState } from "react";
import { WebViewer } from "@rerun-io/web-viewer";
import {
  planRerunSourceTransition,
  type GovernedRerunSource,
  type OpenedRerunSources,
} from "../rerunSources";

type ViewerStatus =
  | { state: "loading"; delayed: boolean }
  | { state: "open" }
  | { state: "error"; message: string };

function synchronizeSources(
  viewer: WebViewer,
  opened: OpenedRerunSources,
  desired: GovernedRerunSource
) {
  const transition = planRerunSourceTransition(opened, desired);
  if (transition.archiveUrlsToOpen.length > 0) {
    viewer.open(transition.archiveUrlsToOpen, { follow_if_http: false });
  }
  if (transition.liveUrlToOpen) {
    viewer.open(transition.liveUrlToOpen, { follow_if_http: true });
  }
  if (transition.urlsToClose.length > 0) viewer.close(transition.urlsToClose);
  opened.archiveUrls = transition.next.archiveUrls;
  opened.liveUrl = transition.next.liveUrl;
}

export default function GovernedRerunViewer({
  recordingId,
  source,
}: {
  recordingId: string;
  source: GovernedRerunSource;
}) {
  const host = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<WebViewer | undefined>(undefined);
  const desiredSourceRef = useRef(source);
  const openedSourcesRef = useRef<OpenedRerunSources>({ archiveUrls: new Set() });
  const [status, setStatus] = useState<ViewerStatus>({
    state: "loading",
    delayed: false,
  });

  useEffect(() => {
    desiredSourceRef.current = source;
    const viewer = viewerRef.current;
    if (!viewer) return;
    try {
      synchronizeSources(viewer, openedSourcesRef.current, source);
    } catch (cause: unknown) {
      const message = cause instanceof Error ? cause.message : "Rerun playback failed";
      console.error("Governed Rerun source update failed", cause);
      queueMicrotask(() => setStatus({ state: "error", message }));
    }
  }, [source]);

  useEffect(() => {
    const viewer = new WebViewer();
    let active = true;
    let removeOpenListener: (() => void) | undefined;
    let delayedNotice: number | undefined;
    openedSourcesRef.current = { archiveUrls: new Set() };
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
        viewerRef.current = viewer;
        synchronizeSources(viewer, openedSourcesRef.current, desiredSourceRef.current);
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
      viewerRef.current = undefined;
      if (delayedNotice !== undefined) window.clearTimeout(delayedNotice);
      removeOpenListener?.();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
    };
  }, [recordingId]);

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
              : source.liveUrl
                ? "Connecting to live capture"
                : "Preparing replay"}
          </strong>
          <span>
            {status.delayed
              ? "Authorized data is still streaming into Rerun. Playback will open automatically; large recordings can take longer."
              : source.liveUrl
              ? "Opening authorized history, then following newly durable RRD batches."
              : "Opening the complete authorized, footer-indexed recording history."}
          </span>
        </div>
      ) : null}
    </div>
  );
}
