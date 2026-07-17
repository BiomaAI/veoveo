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
  const [status, setStatus] = useState<ViewerStatus>({
    loaded: 0,
    total: segments.length,
  });

  useEffect(() => {
    const controller = new AbortController();
    const viewer = new WebViewer();
    const channels: LogChannel[] = [];
    let disposed = false;

    void (async () => {
      await viewer.start(null, host.current, {
        width: "100%",
        height: "100%",
        hide_welcome_screen: true,
        allow_fullscreen: true,
      });
      if (disposed) {
        viewer.stop();
        return;
      }
      setStatus({ loaded: 0, total: segments.length });

      for (const segment of segments) {
        const bytes = await fetchSegment(segment, controller.signal);
        if (disposed) return;

        const channel = viewer.open_channel(
          `recording ${recordingId} · segment ${segment.ordinal + 1}`
        );
        channels.push(channel);
        channel.send_rrd(bytes);
        channel.close();
        setStatus((current) => ({
          loaded: current.loaded + 1,
          total: current.total,
        }));
      }
    })().catch((cause: unknown) => {
      if (!disposed && !controller.signal.aborted) {
        const message = cause instanceof Error ? cause.message : "Rerun playback failed";
        console.error("Authenticated Rerun playback failed", cause);
        setStatus({ loaded: 0, total: segments.length, error: message });
      }
    });

    return () => {
      disposed = true;
      controller.abort();
      for (const channel of channels) channel.close();
      viewer.stop();
    };
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
