import { useEffect, useRef, useState } from "react";
import { WebViewer } from "@rerun-io/web-viewer";
import { loadRerunMapViewerOptions } from "../rerunMap";

export default function GovernedRerunArtifactViewer({
  artifactId,
  url,
}: {
  artifactId: string;
  url: string;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string>();
  const [mapError, setMapError] = useState<string>();

  useEffect(() => {
    const viewer = new WebViewer();
    const controller = new AbortController();

    void loadRerunMapViewerOptions()
      .catch((cause: unknown) => ({
        provider: "mapbox" as const,
        options: {},
        mapError:
          cause instanceof Error ? cause.message : "Map provider configuration failed",
      }))
      .then((mapSetup) => {
        setMapError(mapSetup.mapError);
        return viewer.start(null, host.current, {
          width: "100%",
          height: "100%",
          hide_welcome_screen: true,
          allow_fullscreen: true,
          ...mapSetup.options,
        });
      })
      .then(async () => {
        const response = await fetch(url, {
          credentials: "same-origin",
          headers: { Accept: "application/vnd.rerun.rrd" },
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`Artifact RRD returned ${response.status}`);
        }
        const channel = viewer.open_channel(`artifact ${artifactId}`);
        channel.send_rrd(new Uint8Array(await response.arrayBuffer()));
      })
      .catch((cause: unknown) => {
        if (controller.signal.aborted) return;
        const message = cause instanceof Error ? cause.message : "Rerun artifact preview failed";
        console.error("Governed Rerun artifact failed", cause);
        setError(message);
      });

    return () => {
      controller.abort();
      try {
        viewer.stop();
      } catch (cause) {
        console.warn("Rerun artifact cleanup failed", cause);
      }
    };
  }, [artifactId, url]);

  return (
    <div className="rerun-web-viewer">
      <div ref={host} className="rerun-web-viewer-host" />
      {error && (
        <div className="recording-viewer-state recording-viewer-overlay recording-viewer-error">
          <strong>Rerun could not open this artifact.</strong>
          <span>{error}</span>
        </div>
      )}
      {mapError ? (
        <div className="recording-viewer-map-error" role="alert">
          <strong>Map background unavailable.</strong>
          <span>{mapError}</span>
        </div>
      ) : null}
    </div>
  );
}
