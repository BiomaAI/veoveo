import { useEffect, useRef, useState } from "react";
import { WebViewer } from "@rerun-io/web-viewer";
import {
  planRerunSourceTransition,
  type GovernedRerunSource,
  type OpenedRerunSources,
} from "../rerunSources";
import { installConsoleRecordingRrdFetch } from "../recordingRrdFetch";
import {
  loadRerunMapViewerOptions,
  mapProviderCompatibilityError,
} from "../rerunMap";
import {
  clearConsoleRerunLiveProxyRoute,
  setConsoleRerunLiveProxyRoute,
} from "../rerunLiveProxy";

type ViewerStatus =
  | { state: "loading" }
  | { state: "open" }
  | { state: "error"; message: string };

async function synchronizeSources(
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
  if (transition.receiverUrlToOpen) {
    if (desired.receiver.kind === "live") {
      await setConsoleRerunLiveProxyRoute(desired.receiver.route);
    } else if (opened.receiver?.kind === "live") {
      await clearConsoleRerunLiveProxyRoute();
    }
  }
  if (transition.blueprintUrlToOpen) viewer.open(transition.blueprintUrlToOpen);
  if (transition.receiverUrlToOpen) viewer.open(transition.receiverUrlToOpen);
  opened.redapToken = transition.next.redapToken;
  opened.receiver = transition.next.receiver;
  opened.blueprintUrl = transition.next.blueprintUrl;
  opened.blueprintMapProvider = transition.next.blueprintMapProvider;
  return transition;
}

export default function GovernedRerunViewer({
  recordingId,
  source,
}: {
  recordingId: string;
  source: GovernedRerunSource;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [viewerInstance] = useState(() => crypto.randomUUID());
  const viewerRef = useRef<WebViewer | undefined>(undefined);
  const desiredSourceRef = useRef(source);
  const openedSourcesRef = useRef<OpenedRerunSources>({});
  const sourceSynchronizationRef = useRef<Promise<void>>(Promise.resolve());
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

  useEffect(() => {
    desiredSourceRef.current = source;
    const viewer = viewerRef.current;
    if (!viewer) return;
    sourceSynchronizationRef.current = sourceSynchronizationRef.current
      .then(async () => {
        const transition = await synchronizeSources(
          viewer,
          openedSourcesRef.current,
          source
        );
        if (source.receiver.kind === "live" && transition.receiverUrlToOpen && host.current) {
          const connections = Number(host.current.dataset.rerunLiveConnectionCount ?? 0) + 1;
          host.current.dataset.rerunLiveConnectionCount = String(connections);
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
      })
      .catch((cause: unknown) => {
        const message = cause instanceof Error ? cause.message : "Rerun playback failed";
        console.error("Governed Rerun source update failed", cause);
        setStatus({ state: "error", message });
      });
  }, [source]);

  useEffect(() => {
    const viewer = new WebViewer();
    const releaseRecordingRrdFetch = installConsoleRecordingRrdFetch();
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
          // Rerun 0.35 supports this hardware backend explicitly. It keeps presentation
          // off the WebGPU queue shared with long-running CUDA/RTX workloads without
          // changing Rerun's native MessageProxy transport.
          render_backend: "webgl",
          hide_welcome_screen: true,
          allow_fullscreen: true,
          fallback_token: desiredSourceRef.current.redapToken,
          ...mapSetup.options,
        });
      })
      .then(() => {
        if (!active) return;
        removeOpenListener = viewer.on("recording_open", (event) => {
          if (!active) return;
          removeOpenListener?.();
          removeOpenListener = undefined;
          if (host.current) {
            host.current.dataset.rerunRecordingId = event.recording_id;
            host.current.dataset.rerunViewerState = "open";
          }
          setStatus({ state: "open" });
        });
        removeTimeUpdateListener = viewer.on("time_update", (event) => {
          if (!active || desiredSourceRef.current.receiver.kind !== "live") return;
          const timeline =
            host.current?.dataset.rerunTimeline ||
            viewer.get_active_timeline(event.recording_id);
          if (!timeline) return;
          if (!host.current) return;
          const range = viewer.get_time_range(event.recording_id, timeline);
          const updates = Number(host.current.dataset.rerunTimeUpdateCount ?? 0) + 1;
          host.current.dataset.rerunRecordingId = event.recording_id;
          host.current.dataset.rerunTimeline = timeline;
          host.current.dataset.rerunCurrentTime = String(event.time);
          if (range) {
            host.current.dataset.rerunNewestTime = String(range.max);
            if (timeline === "simulation_time") {
              host.current.dataset.rerunLiveLagSeconds = String(
                Math.max(0, range.max - event.time) / 1_000_000_000
              );
            } else {
              delete host.current.dataset.rerunLiveLagSeconds;
            }
          }
          host.current.dataset.rerunTimeUpdateCount = String(updates);
        });
        viewerRef.current = viewer;
        sourceSynchronizationRef.current = sourceSynchronizationRef.current.then(async () => {
          const transition = await synchronizeSources(
            viewer,
            openedSourcesRef.current,
            desiredSourceRef.current
          );
          if (
            desiredSourceRef.current.receiver.kind === "live" &&
            transition.receiverUrlToOpen &&
            host.current
          ) {
            host.current.dataset.rerunLiveConnectionCount = "1";
          }
        });
        return sourceSynchronizationRef.current;
      })
      .catch((cause: unknown) => {
        if (!active) return;
        const message = cause instanceof Error ? cause.message : "Rerun playback failed";
        console.error("Governed Rerun source failed", cause);
        setStatus({ state: "error", message });
      });

    return () => {
      active = false;
      viewerRef.current = undefined;
      mapSetupRef.current = undefined;
      removeOpenListener?.();
      removeTimeUpdateListener?.();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
      releaseRecordingRrdFetch();
      void clearConsoleRerunLiveProxyRoute();
    };
  }, [recordingId]);

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
