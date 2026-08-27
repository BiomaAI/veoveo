import { useEffect, useRef, useState } from "react";
import { WebViewer, type LogChannel } from "@rerun-io/web-viewer";
import { installConsoleRecordingRrdFetch } from "../recordingRrdFetch";
import { resetRerunEmbeddedViewerState } from "../rerunEmbeddedState";
import {
  connectConsoleRerunLiveChannel,
  type RerunLiveConnection,
} from "../rerunLiveChannel";
import {
  planRerunSourceTransition,
  type GovernedRerunSource,
  type OpenedRerunSources,
} from "../rerunSources";
import {
  loadRerunMapViewerOptions,
  mapProviderCompatibilityError,
} from "../rerunMap";

type ViewerStatus =
  | { state: "loading" }
  | { state: "open" }
  | { state: "error"; message: string };

const ARCHIVE_TIMELINE = "simulation_time";
const ARCHIVE_SEEK_ATTEMPTS = 300;
const ARCHIVE_SEEK_RETRY_MILLISECONDS = 100;

interface LiveRuntime {
  channel?: LogChannel;
  connection?: RerunLiveConnection;
  route?: string;
  disconnected: boolean;
  seeded: boolean;
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : "Rerun playback failed";
}

function closeLiveConnection(runtime: LiveRuntime): void {
  const connection = runtime.connection;
  runtime.connection = undefined;
  runtime.route = undefined;
  connection?.close();
}

function startLiveConnection(
  viewer: WebViewer,
  runtime: LiveRuntime,
  route: string,
  host: HTMLDivElement | null,
  reportConnected: () => void,
  reportError: (message: string) => void
): void {
  const start = runtime.seeded ? "resume-head" : "bootstrap";
  closeLiveConnection(runtime);
  const channel = runtime.channel ?? viewer.open_channel("governed-recording-live");
  runtime.channel = channel;
  runtime.route = route;
  runtime.disconnected = false;
  if (host) host.dataset.rerunLiveState = "connecting";

  const connection = connectConsoleRerunLiveChannel(channel, route, start, {
    onConnected() {
      if (runtime.connection !== connection || !host) return;
      runtime.disconnected = false;
      host.dataset.rerunLiveState = "connected";
      host.dataset.rerunLiveConnectionCount = String(
        Number(host.dataset.rerunLiveConnectionCount ?? 0) + 1
      );
      reportConnected();
    },
    onFrame(byteLength) {
      if (runtime.connection !== connection || !host) return;
      runtime.seeded = true;
      host.dataset.rerunLiveFrameCount = String(
        Number(host.dataset.rerunLiveFrameCount ?? 0) + 1
      );
      host.dataset.rerunLiveNewestFrameBytes = String(byteLength);
    },
  });
  runtime.connection = connection;
  void connection.done.then(
    () => {
      if (runtime.connection !== connection) return;
      runtime.connection = undefined;
      runtime.disconnected = true;
      if (host) host.dataset.rerunLiveState = "ended";
      reportError("Live recording delivery ended. Reconnect after connectivity is restored.");
    },
    (cause: unknown) => {
      if (runtime.connection !== connection) return;
      runtime.connection = undefined;
      runtime.disconnected = true;
      if (host) host.dataset.rerunLiveState = "error";
      reportError(errorMessage(cause));
    }
  );
}

async function synchronizeSources(
  viewer: WebViewer,
  opened: OpenedRerunSources,
  desired: GovernedRerunSource,
  live: LiveRuntime,
  host: HTMLDivElement | null,
  reportConnected: () => void,
  reportError: (message: string) => void
) {
  const transition = planRerunSourceTransition(opened, desired);
  if (transition.credentialsChanged) {
    viewer.set_credentials(desired.redapToken, "");
  }
  if (transition.urlsToCloseBeforeOpen.length > 0) {
    viewer.close(transition.urlsToCloseBeforeOpen);
  }
  if (transition.closeLiveConnection) closeLiveConnection(live);
  if (transition.blueprintUrlToOpen) viewer.open(transition.blueprintUrlToOpen);
  if (transition.archiveUrlToOpen) viewer.open(transition.archiveUrlToOpen);
  if (transition.liveRouteToOpen) {
    startLiveConnection(
      viewer,
      live,
      transition.liveRouteToOpen,
      host,
      reportConnected,
      reportError
    );
  }
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
  const liveRuntimeRef = useRef<LiveRuntime>({ disconnected: false, seeded: false });
  const mapSetupRef = useRef<
    | {
        provider: "openStreetMap" | "mapbox";
        configurationError?: string;
      }
    | undefined
  >(undefined);
  const [status, setStatus] = useState<ViewerStatus>({ state: "loading" });
  const [mapError, setMapError] = useState<string>();

  const reportPlaybackError = (message: string) => {
    setStatus({ state: "error", message });
  };

  useEffect(() => {
    desiredSourceRef.current = source;
    const viewer = viewerRef.current;
    if (!viewer) return;
    sourceSynchronizationRef.current = sourceSynchronizationRef.current
      .then(async () => {
        await synchronizeSources(
          viewer,
          openedSourcesRef.current,
          source,
          liveRuntimeRef.current,
          host.current,
          () => setStatus({ state: "open" }),
          reportPlaybackError
        );
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
        console.error("Governed Rerun source update failed", cause);
        reportPlaybackError(errorMessage(cause));
      });
  }, [source]);

  useEffect(() => {
    resetRerunEmbeddedViewerState();
    const viewer = new WebViewer();
    const releaseRecordingRrdFetch = installConsoleRecordingRrdFetch();
    let active = true;
    let removeOpenListener: (() => void) | undefined;
    let removeTimeUpdateListener: (() => void) | undefined;
    const archiveSeekTimers = new Set<number>();
    openedSourcesRef.current = { redapToken: desiredSourceRef.current.redapToken };
    liveRuntimeRef.current = { disconnected: false, seeded: false };

    const seekArchiveToLatest = (recordingId: string, attemptsRemaining: number) => {
      if (
        !active ||
        desiredSourceRef.current.receiver.kind !== "archive" ||
        host.current?.dataset.rerunArchiveState === "latest"
      ) {
        return;
      }
      const range = viewer.get_time_range(recordingId, ARCHIVE_TIMELINE);
      if (!range || !Number.isFinite(range.max)) {
        if (attemptsRemaining <= 1) {
          if (host.current) host.current.dataset.rerunArchiveState = "timeline-unavailable";
          return;
        }
        const timer = window.setTimeout(() => {
          archiveSeekTimers.delete(timer);
          seekArchiveToLatest(recordingId, attemptsRemaining - 1);
        }, ARCHIVE_SEEK_RETRY_MILLISECONDS);
        archiveSeekTimers.add(timer);
        return;
      }
      viewer.set_active_recording_id(recordingId);
      viewer.set_active_timeline(recordingId, ARCHIVE_TIMELINE);
      viewer.set_current_time(recordingId, ARCHIVE_TIMELINE, range.max);
      if (host.current) {
        host.current.dataset.rerunRecordingId = recordingId;
        host.current.dataset.rerunTimeline = ARCHIVE_TIMELINE;
        host.current.dataset.rerunCurrentTime = String(range.max);
        host.current.dataset.rerunNewestTime = String(range.max);
        host.current.dataset.rerunArchiveState = "latest";
      }
    };

    const disconnect = () => {
      const desired = desiredSourceRef.current;
      const runtime = liveRuntimeRef.current;
      if (
        !active ||
        desired.receiver.kind !== "live" ||
        !runtime.connection
      ) {
        return;
      }
      closeLiveConnection(runtime);
      runtime.disconnected = true;
      if (host.current) host.current.dataset.rerunLiveState = "error";
      reportPlaybackError(
        "Live recording delivery is offline. It will reconnect when connectivity returns."
      );
    };
    const reconnect = () => {
      const desired = desiredSourceRef.current;
      const runtime = liveRuntimeRef.current;
      if (
        !active ||
        desired.receiver.kind !== "live" ||
        !runtime.disconnected ||
        !viewerRef.current
      ) {
        return;
      }
      setStatus({ state: "loading" });
      startLiveConnection(
        viewer,
        runtime,
        desired.receiver.route,
        host.current,
        () => setStatus({ state: "open" }),
        reportPlaybackError
      );
    };
    window.addEventListener("offline", disconnect);
    window.addEventListener("online", reconnect);

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
          if (host.current) {
            host.current.dataset.rerunRecordingId = event.recording_id;
            host.current.dataset.rerunViewerState = "open";
          }
          if (desiredSourceRef.current.receiver.kind === "archive") {
            seekArchiveToLatest(event.recording_id, ARCHIVE_SEEK_ATTEMPTS);
          }
          setStatus({ state: "open" });
        });
        removeTimeUpdateListener = viewer.on("time_update", (event) => {
          if (!active || desiredSourceRef.current.receiver.kind !== "live") return;
          const timeline =
            host.current?.dataset.rerunTimeline ||
            viewer.get_active_timeline(event.recording_id);
          if (!timeline || !host.current) return;
          const range = viewer.get_time_range(event.recording_id, timeline);
          host.current.dataset.rerunRecordingId = event.recording_id;
          host.current.dataset.rerunTimeline = timeline;
          host.current.dataset.rerunCurrentTime = String(event.time);
          host.current.dataset.rerunTimeUpdateCount = String(
            Number(host.current.dataset.rerunTimeUpdateCount ?? 0) + 1
          );
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
        });
        viewerRef.current = viewer;
        sourceSynchronizationRef.current = sourceSynchronizationRef.current.then(() =>
          synchronizeSources(
            viewer,
            openedSourcesRef.current,
            desiredSourceRef.current,
            liveRuntimeRef.current,
            host.current,
            () => setStatus({ state: "open" }),
            reportPlaybackError
          ).then(() => undefined)
        );
        return sourceSynchronizationRef.current;
      })
      .catch((cause: unknown) => {
        if (!active) return;
        console.error("Governed Rerun source failed", cause);
        reportPlaybackError(errorMessage(cause));
      });

    return () => {
      active = false;
      window.removeEventListener("offline", disconnect);
      window.removeEventListener("online", reconnect);
      viewerRef.current = undefined;
      mapSetupRef.current = undefined;
      closeLiveConnection(liveRuntimeRef.current);
      liveRuntimeRef.current.channel?.close();
      liveRuntimeRef.current.channel = undefined;
      removeOpenListener?.();
      removeTimeUpdateListener?.();
      for (const timer of archiveSeekTimers) window.clearTimeout(timer);
      archiveSeekTimers.clear();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
      releaseRecordingRrdFetch();
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
              ? "Following the current Rerun stream through an incremental channel."
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
