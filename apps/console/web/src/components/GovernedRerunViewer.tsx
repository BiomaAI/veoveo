import { useEffect, useRef, useState } from "react";
import { WebViewer, type LogChannel } from "@rerun-io/web-viewer";

export interface GovernedRerunSegment {
  ordinal: number;
  byteLength: number;
  url: string;
}

interface ViewerStatus {
  loaded: number;
  total: number;
  error?: string;
}

interface ViewerSession {
  viewer: WebViewer;
  ready: Promise<void>;
  loadedSegments: Set<string>;
  channel?: LogChannel;
}

async function fetchSegment(
  segment: GovernedRerunSegment,
  signal: AbortSignal
): Promise<Uint8Array> {
  const response = await fetch(segment.url, {
    credentials: "same-origin",
    headers: { Accept: "application/vnd.rerun.rrd" },
    signal,
  });
  if (response.status === 401) {
    window.location.assign("/auth/login");
    throw new Error("Authentication required");
  }
  if (response.status === 403) {
    throw new Error("Playback is not permitted by the active recording policy.");
  }
  if (!response.ok) {
    throw new Error(`RRD segment ${segment.ordinal + 1} returned ${response.status}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

export default function GovernedRerunViewer({
  recordingId,
  segments,
}: {
  recordingId: string;
  segments: GovernedRerunSegment[];
}) {
  const host = useRef<HTMLDivElement>(null);
  const session = useRef<ViewerSession>(null);
  const [status, setStatus] = useState<ViewerStatus>({
    loaded: 0,
    total: segments.length,
  });

  useEffect(() => {
    const viewer = new WebViewer();
    const current: ViewerSession = {
      viewer,
      loadedSegments: new Set(),
      ready: viewer.start(null, host.current, {
        width: "100%",
        height: "100%",
        hide_welcome_screen: true,
        allow_fullscreen: true,
      }),
    };
    session.current = current;

    return () => {
      if (session.current === current) session.current = null;
      current.channel?.close();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
    };
  }, [recordingId]);

  useEffect(() => {
    const controller = new AbortController();
    const current = session.current;
    if (!current) return;

    setStatus({
      loaded: current.loadedSegments.size,
      total: segments.length,
    });

    void (async () => {
      await current.ready;
      if (controller.signal.aborted || session.current !== current) return;
      if (!current.viewer.ready) {
        throw new Error("Rerun stopped before the recording channel could be opened.");
      }
      current.channel ??= current.viewer.open_channel(`recording ${recordingId}`);

      for (const segment of [...segments].sort((left, right) => left.ordinal - right.ordinal)) {
        if (current.loadedSegments.has(segment.url)) continue;

        const bytes = await fetchSegment(segment, controller.signal);
        if (controller.signal.aborted || session.current !== current) return;
        if (!current.viewer.ready) {
          throw new Error(
            `Rerun stopped before segment ${segment.ordinal + 1} could be opened.`
          );
        }

        current.channel.send_rrd(bytes);
        current.loadedSegments.add(segment.url);
        setStatus({
          loaded: current.loadedSegments.size,
          total: segments.length,
        });
      }
    })().catch((cause: unknown) => {
      if (!controller.signal.aborted && session.current === current) {
        const message = cause instanceof Error ? cause.message : "Rerun playback failed";
        console.error("Authenticated Rerun playback failed", cause);
        setStatus({
          loaded: current.loadedSegments.size,
          total: segments.length,
          error: message,
        });
      }
    });

    return () => controller.abort();
  }, [recordingId, segments]);

  return (
    <div className="rerun-web-viewer">
      <div ref={host} className="rerun-web-viewer-host" />
      {status.error ? (
        <div className="recording-viewer-state recording-viewer-overlay recording-viewer-error">
          <strong>Rerun could not open this recording.</strong>
          <span>{status.error}</span>
        </div>
      ) : status.loaded < status.total ? (
        <div className="recording-viewer-state recording-viewer-overlay">
          <div className="loading-mark" />
          <strong>Loading governed RRD data</strong>
          <span>
            Segment {Math.min(status.loaded + 1, status.total)} of {status.total}
          </span>
        </div>
      ) : null}
    </div>
  );
}
