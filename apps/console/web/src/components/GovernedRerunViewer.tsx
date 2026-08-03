import { useEffect, useRef, useState } from "react";
import { WebViewer } from "@rerun-io/web-viewer";
import {
  planRerunSourceTransition,
  type GovernedRerunSource,
  type OpenedRerunSources,
} from "../rerunSources";
import { installConsoleRecordingRrdFetch } from "../recordingLiveFetch";
import {
  loadRerunMapViewerOptions,
  mapProviderCompatibilityError,
} from "../rerunMap";

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
  if (transition.urlsToCloseBeforeOpen.length > 0) {
    viewer.close(transition.urlsToCloseBeforeOpen);
  }
  if (transition.blueprintUrlToOpen) viewer.open(transition.blueprintUrlToOpen);
  if (transition.receiverUrlToOpen) viewer.open(transition.receiverUrlToOpen);
  opened.redapToken = transition.next.redapToken;
  opened.receiver = transition.next.receiver;
  opened.blueprintUrl = transition.next.blueprintUrl;
  opened.blueprintMapProvider = transition.next.blueprintMapProvider;
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
    delayed: false,
  });
  const [mapError, setMapError] = useState<string>();

  useEffect(() => {
    desiredSourceRef.current = source;
    const viewer = viewerRef.current;
    if (!viewer) return;
    if (openedSourcesRef.current.redapToken !== source.redapToken) {
      return;
    }
    try {
      synchronizeSources(viewer, openedSourcesRef.current, source);
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
  }, [source]);

  useEffect(() => {
    const viewer = new WebViewer();
    const releaseRecordingRrdFetch = installConsoleRecordingRrdFetch();
    let active = true;
    let removeOpenListener: (() => void) | undefined;
    let delayedNotice: number | undefined;
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
      mapSetupRef.current = undefined;
      if (delayedNotice !== undefined) window.clearTimeout(delayedNotice);
      removeOpenListener?.();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun cleanup failed after the viewer stopped", cause);
      }
      releaseRecordingRrdFetch();
    };
  }, [recordingId, source.redapToken]);

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
              : source.receiver.kind === "live"
                ? "Connecting to live capture"
                : "Preparing replay"}
          </strong>
          <span>
            {status.delayed
              ? "Rerun is still fetching the selected time window. Playback will open automatically."
              : source.receiver.kind === "live"
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
