import { useCallback, useEffect, useRef, useState } from "react";
import { WebViewer, type LogChannel } from "@rerun-io/web-viewer";
import {
  planRerunSourceTransition,
  type GovernedRerunSource,
  type OpenedRerunSources,
} from "../rerunSources";
import { installConsoleRecordingBlueprintFetch } from "../recordingBlueprintFetch";
import { pumpRerunLiveFrames } from "../rerunLiveChannel";
import {
  loadRerunMapViewerOptions,
  mapProviderCompatibilityError,
} from "../rerunMap";

type ViewerStatus =
  | { state: "loading" }
  | { state: "open" }
  | { state: "error"; message: string };

function synchronizeSources(
  viewer: WebViewer,
  opened: OpenedRerunSources,
  desired: GovernedRerunSource
) {
  const transition = planRerunSourceTransition(opened, desired);
  if (transition.credentialsChanged) {
    viewer.set_credentials(desired.redapToken, "");
  }
  if (transition.urlsToCloseBeforeOpen.length > 0) {
    viewer.close(transition.urlsToCloseBeforeOpen);
  }
  if (transition.blueprintUrlToOpen) viewer.open(transition.blueprintUrlToOpen);
  if (transition.receiverUrlToOpen && desired.receiver.kind === "archive") {
    viewer.open(transition.receiverUrlToOpen);
  }
  opened.redapToken = transition.next.redapToken;
  opened.receiver = transition.next.receiver;
  opened.blueprintUrl = transition.next.blueprintUrl;
  opened.blueprintMapProvider = transition.next.blueprintMapProvider;
}

export default function GovernedRerunViewer({
  recordingId,
  source,
  onLiveReceiverEnded,
}: {
  recordingId: string;
  source: GovernedRerunSource;
  onLiveReceiverEnded?: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [viewerInstance] = useState(() => crypto.randomUUID());
  const viewerRef = useRef<WebViewer | undefined>(undefined);
  const liveChannelRef = useRef<LogChannel | undefined>(undefined);
  const liveAbortRef = useRef<AbortController | undefined>(undefined);
  const liveTotalsRef = useRef({
    frames: 0,
    payloadBytes: 0,
    connections: 0,
    sendRrdTotalMs: 0,
    sendRrdMaximumMs: 0,
  });
  const viewerOpenRef = useRef(false);
  const desiredSourceRef = useRef(source);
  const liveReceiverEndedRef = useRef(onLiveReceiverEnded);
  const openedSourcesRef = useRef<OpenedRerunSources>({});
  const mapSetupRef = useRef<
    | {
        provider: "openStreetMap" | "mapbox";
        configurationError?: string;
      }
    | undefined
  >(undefined);
  const [status, setStatus] = useState<ViewerStatus>({
    state: "loading",
  });
  const [mapError, setMapError] = useState<string>();
  const startLiveReceiver = useCallback((url: string, channel: LogChannel) => {
    liveAbortRef.current?.abort();
    const abort = new AbortController();
    liveAbortRef.current = abort;
    const base = { ...liveTotalsRef.current };
    liveTotalsRef.current.connections += 1;
    if (host.current) {
      host.current.dataset.rerunLiveConnectionCount = String(
        liveTotalsRef.current.connections
      );
    }
    void pumpRerunLiveFrames(url, channel, abort.signal, (stats) => {
      if (!host.current) return;
      liveTotalsRef.current.frames = base.frames + stats.frames;
      liveTotalsRef.current.payloadBytes = base.payloadBytes + stats.payloadBytes;
      liveTotalsRef.current.sendRrdTotalMs =
        base.sendRrdTotalMs + stats.sendRrdTotalMs;
      liveTotalsRef.current.sendRrdMaximumMs = Math.max(
        base.sendRrdMaximumMs,
        stats.sendRrdMaximumMs
      );
      host.current.dataset.rerunLiveFrameCount = String(liveTotalsRef.current.frames);
      host.current.dataset.rerunLivePayloadBytes = String(
        liveTotalsRef.current.payloadBytes
      );
      host.current.dataset.rerunSendRrdTotalMs = String(
        liveTotalsRef.current.sendRrdTotalMs
      );
      host.current.dataset.rerunSendRrdMaximumMs = String(
        liveTotalsRef.current.sendRrdMaximumMs
      );
      const viewer = viewerRef.current;
      const recordingId = host.current.dataset.rerunRecordingId;
      const timeline = host.current.dataset.rerunTimeline;
      if (viewer && recordingId && timeline) {
        const range = viewer.get_time_range(recordingId, timeline);
        if (range) {
          host.current.dataset.rerunNewestTime = String(range.max);
          const current = Number(host.current.dataset.rerunCurrentTime ?? range.max);
          host.current.dataset.rerunLiveLagSeconds = String(
            Math.max(0, range.max - current) / 1_000_000_000
          );
        }
      }
      if (!viewerOpenRef.current) {
        viewerOpenRef.current = true;
        setStatus({ state: "open" });
      }
    })
      .then(() => {
        if (!abort.signal.aborted) liveReceiverEndedRef.current?.();
      })
      .catch((cause: unknown) => {
        if (abort.signal.aborted) return;
        const message = cause instanceof Error ? cause.message : "Live recording failed";
        console.error("Governed Rerun live receiver failed", cause);
        viewerOpenRef.current = false;
        setStatus({ state: "error", message });
        liveReceiverEndedRef.current?.();
      });
  }, []);

  useEffect(() => {
    liveReceiverEndedRef.current = onLiveReceiverEnded;
  }, [onLiveReceiverEnded]);

  useEffect(() => {
    desiredSourceRef.current = source;
    const viewer = viewerRef.current;
    if (!viewer) return;
    try {
      synchronizeSources(viewer, openedSourcesRef.current, source);
      if (source.receiver.kind === "live") {
        const channel = liveChannelRef.current;
        if (!channel) throw new Error("Rerun live channel is unavailable.");
        startLiveReceiver(source.receiver.url, channel);
      }
      const mapSetup = mapSetupRef.current;
      if (mapSetup) {
        setMapError(
          mapSetup.configurationError ??
            mapProviderCompatibilityError(
              mapSetup.provider,
              source.blueprintMapProvider
            )
        );
      }
    } catch (cause: unknown) {
      const message = cause instanceof Error ? cause.message : "Rerun playback failed";
      console.error("Governed Rerun source update failed", cause);
      queueMicrotask(() => setStatus({ state: "error", message }));
    }
  }, [source, startLiveReceiver]);

  useEffect(() => {
    const viewer = new WebViewer();
    const releaseRecordingBlueprintFetch = installConsoleRecordingBlueprintFetch();
    let active = true;
    let removeOpenListener: (() => void) | undefined;
    let removeTimeUpdateListener: (() => void) | undefined;
    openedSourcesRef.current = {
      redapToken: desiredSourceRef.current.redapToken,
    };
    void loadRerunMapViewerOptions()
      .catch((cause: unknown) => ({
        provider: "mapbox" as const,
        options: {},
        mapError:
          cause instanceof Error ? cause.message : "Map provider configuration failed",
      }))
      .then((mapSetup) => {
        mapSetupRef.current = {
          provider: mapSetup.provider,
          configurationError: mapSetup.mapError,
        };
        const providerError =
          mapSetup.mapError ??
          mapProviderCompatibilityError(
            mapSetup.provider,
            desiredSourceRef.current.blueprintMapProvider
          );
        setMapError(providerError);
        return viewer.start(null, host.current, {
          width: "100%",
          height: "100%",
          hide_welcome_screen: true,
          allow_fullscreen: true,
          fallback_token: desiredSourceRef.current.redapToken,
          ...mapSetup.options,
        });
      })
      .then(() => {
        if (!active) return;
        if (desiredSourceRef.current.receiver.kind === "live") {
          liveTotalsRef.current = {
            frames: 0,
            payloadBytes: 0,
            connections: 0,
            sendRrdTotalMs: 0,
            sendRrdMaximumMs: 0,
          };
          liveChannelRef.current = viewer.open_channel("governed-recording-live");
        }
        removeOpenListener = viewer.on("recording_open", (event) => {
          if (!active) return;
          removeOpenListener?.();
          removeOpenListener = undefined;
          if (host.current) {
            host.current.dataset.rerunRecordingId = event.recording_id;
            host.current.dataset.rerunViewerState = "open";
          }
          viewerOpenRef.current = true;
          setStatus({ state: "open" });
        });
        removeTimeUpdateListener = viewer.on("time_update", (event) => {
          if (!active || desiredSourceRef.current.receiver.kind !== "live") return;
          const timeline =
            host.current?.dataset.rerunTimeline ||
            viewer.get_active_timeline(event.recording_id);
          if (!timeline) return;
          if (!host.current) return;
          const updates = Number(host.current.dataset.rerunTimeUpdateCount ?? 0) + 1;
          host.current.dataset.rerunRecordingId = event.recording_id;
          host.current.dataset.rerunTimeline = timeline;
          host.current.dataset.rerunCurrentTime = String(event.time);
          const newest = Number(host.current.dataset.rerunNewestTime ?? event.time);
          host.current.dataset.rerunLiveLagSeconds = String(
            Math.max(0, newest - event.time) / 1_000_000_000
          );
          host.current.dataset.rerunTimeUpdateCount = String(updates);
        });
        viewerRef.current = viewer;
        synchronizeSources(viewer, openedSourcesRef.current, desiredSourceRef.current);
        const receiver = desiredSourceRef.current.receiver;
        if (receiver.kind === "live") {
          startLiveReceiver(receiver.url, liveChannelRef.current!);
        }
      })
      .catch((cause: unknown) => {
        if (!active) return;
        const message = cause instanceof Error ? cause.message : "Rerun playback failed";
        console.error("Governed Rerun source failed", cause);
        setStatus({ state: "error", message });
      });

    return () => {
      active = false;
      liveAbortRef.current?.abort();
      liveAbortRef.current = undefined;
      liveChannelRef.current?.close();
      liveChannelRef.current = undefined;
      viewerRef.current = undefined;
      viewerOpenRef.current = false;
      mapSetupRef.current = undefined;
      removeOpenListener?.();
      removeTimeUpdateListener?.();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
      releaseRecordingBlueprintFetch();
    };
  }, [recordingId, source.receiver.kind, startLiveReceiver]);

  return (
    <div className="rerun-web-viewer">
      <div
        ref={host}
        className="rerun-web-viewer-host"
        data-rerun-viewer-instance={viewerInstance}
        data-rerun-viewer-state="starting"
      />
      {status.state === "error" ? (
        <div className="recording-viewer-state recording-viewer-overlay recording-viewer-error">
          <strong>Rerun could not open this recording.</strong>
          <span>{status.message}</span>
        </div>
      ) : status.state === "loading" ? (
        <div className="recording-viewer-state recording-viewer-overlay">
          <div className="loading-mark" />
          <strong>
            {source.receiver.kind === "live"
                ? "Connecting to live capture"
                : "Preparing replay"}
          </strong>
          <span>
            {source.receiver.kind === "live"
              ? "Following bounded live history while immutable layers remain lazy."
              : "Opening the recording catalog; Rerun fetches chunks as the active view needs them."}
          </span>
        </div>
      ) : null}
      {mapError ? (
        <div className="recording-viewer-map-error" role="alert">
          <strong>Map background unavailable.</strong>
          <span>{mapError}</span>
        </div>
      ) : null}
    </div>
  );
}
