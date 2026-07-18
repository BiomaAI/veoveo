import {
  Component,
  Suspense,
  lazy,
  useEffect,
  useMemo,
  useState,
  type ErrorInfo,
  type ReactNode,
} from "react";
import { Check, Copy, FileStack, Play, RefreshCw, Search } from "lucide-react";
import {
  loadRecordingPlayback,
  recordingLiveSegmentUrl,
  recordingReplayUrl,
} from "../api";
import { EmptyState, SectionHeader, StatusPill } from "../components/primitives";
import { formatBytes, formatDate } from "../format";
import type {
  InstallationSnapshot,
  RecordingPlaybackManifest,
  RecordingSummary,
} from "../types";

const GovernedRerunViewer = lazy(() => import("../components/GovernedRerunViewer"));

type RecordingStateFilter = "all" | RecordingSummary["state"];

const lifecycleDetail: Record<RecordingSummary["state"], string> = {
  live: "Receiving data",
  ready: "Capture complete",
  sealing: "Publishing governed artifacts",
  sealed: "Published and immutable",
  interrupted: "Producer stopped without a clean boundary",
  failed: "Recording processing failed",
};

function initialRecordingSelection(
  recordings: RecordingSummary[],
  requestedId?: string
): string | undefined {
  const requested = requestedId
    ? recordings.find(
        (recording) =>
          recording.id === requestedId || recording.recordingKey === requestedId
      )
    : undefined;
  return (
    requested?.id ??
    recordings.find(
      (recording) =>
        (recording.state === "ready" || recording.state === "sealed") &&
        recording.playableSegmentCount > 0
    )?.id ??
    recordings.find((recording) => recording.playableSegmentCount > 0)?.id ??
    recordings[0]?.id
  );
}

class ViewerBoundary extends Component<
  { children: ReactNode; recordingId: string },
  { error?: Error }
> {
  state: { error?: Error } = {};

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Rerun viewer failed", error, info);
  }

  componentDidUpdate(previous: Readonly<{ children: ReactNode; recordingId: string }>) {
    if (previous.recordingId !== this.props.recordingId && this.state.error) {
      this.setState({ error: undefined });
    }
  }

  render() {
    if (this.state.error) {
      return (
        <div className="recording-viewer-state recording-viewer-error">
          <strong>Rerun could not open this recording.</strong>
          <span>{this.state.error.message}</span>
        </div>
      );
    }
    return this.props.children;
  }
}

export function RecordingsView({
  snapshot,
  initialRecordingId,
  onRecordingSelect,
}: {
  snapshot: InstallationSnapshot;
  initialRecordingId?: string;
  onRecordingSelect: (recordingId: string) => void;
}) {
  const firstSelectedId = initialRecordingSelection(
    snapshot.recordings,
    initialRecordingId
  );
  const [selectedId, setSelectedId] = useState<string | undefined>(firstSelectedId);
  const [query, setQuery] = useState("");
  const [stateFilter, setStateFilter] = useState<RecordingStateFilter>("all");
  const [manifest, setManifest] = useState<RecordingPlaybackManifest>();
  const [loading, setLoading] = useState(Boolean(firstSelectedId));
  const [playbackError, setPlaybackError] = useState<string>();
  const [reloadToken, setReloadToken] = useState(0);
  const [copied, setCopied] = useState(false);

  const recordings = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return snapshot.recordings.filter(
      (recording) =>
        (stateFilter === "all" || recording.state === stateFilter) &&
        (!needle ||
          recording.recordingKey.toLowerCase().includes(needle) ||
          recording.application.toLowerCase().includes(needle) ||
          recording.id.toLowerCase().includes(needle))
    );
  }, [query, snapshot.recordings, stateFilter]);

  const resolvedSelectedId = snapshot.recordings.some(
    (recording) => recording.id === selectedId
  )
    ? selectedId
    : initialRecordingSelection(snapshot.recordings);
  const selected = snapshot.recordings.find(
    (recording) => recording.id === resolvedSelectedId
  );

  useEffect(() => {
    if (!resolvedSelectedId) return;
    const controller = new AbortController();
    void loadRecordingPlayback(resolvedSelectedId, controller.signal)
      .then((value) => {
        setManifest(value);
        setPlaybackError(undefined);
      })
      .catch((cause: unknown) => {
        if (!controller.signal.aborted) {
          setPlaybackError(cause instanceof Error ? cause.message : "Playback failed");
          setManifest(undefined);
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [
    reloadToken,
    selected?.playableByteLength,
    selected?.playableSegmentCount,
    selected?.state,
    resolvedSelectedId,
  ]);

  const selectRecording = (recordingId: string) => {
    if (recordingId === resolvedSelectedId) return;
    setManifest(undefined);
    setPlaybackError(undefined);
    setLoading(true);
    setSelectedId(recordingId);
    onRecordingSelect(recordingId);
  };

  const reloadPlayback = () => {
    setManifest(undefined);
    setPlaybackError(undefined);
    setLoading(true);
    setReloadToken((value) => value + 1);
  };

  const copyRecordingUri = async () => {
    if (!selected) return;
    await navigator.clipboard.writeText(`recording://recordings/${selected.id}`);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  const playbackSource = useMemo(() => {
    if (!manifest) return undefined;
    if (manifest.live_segment) {
      return {
        mode: "live" as const,
        url: recordingLiveSegmentUrl(
          manifest.recording_id,
          manifest.playback_ticket,
          manifest.live_segment.segment_id
        ),
      };
    }
    if (manifest.segments.length > 0) {
      return {
        mode: "replay" as const,
        url: recordingReplayUrl(manifest.recording_id, manifest.playback_ticket),
      };
    }
    return undefined;
  }, [manifest]);

  return (
    <div className="recordings-workspace">
      <section className="panel recordings-browser">
        <SectionHeader title="Recordings" count={recordings.length} />
        <div className="recording-filters">
          <label className="search-control">
            <Search size={14} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search recordings"
              aria-label="Search recordings"
            />
          </label>
          <label className="filter-control">
            <select
              value={stateFilter}
              onChange={(event) => setStateFilter(event.target.value as RecordingStateFilter)}
              aria-label="Recording state"
            >
              <option value="all">All states</option>
              <option value="live">Live</option>
              <option value="ready">Ready</option>
              <option value="sealed">Sealed</option>
              <option value="interrupted">Interrupted</option>
              <option value="failed">Failed</option>
            </select>
          </label>
        </div>
        <div className="recording-list">
          {recordings.map((recording) => (
            <button
              key={recording.id}
              className={recording.id === resolvedSelectedId ? "recording-card recording-card-active" : "recording-card"}
              onClick={() => selectRecording(recording.id)}
              aria-pressed={recording.id === resolvedSelectedId}
            >
              <div className="recording-card-head">
                <strong>{recording.recordingKey}</strong>
                <StatusPill value={recording.state} />
              </div>
              <span>{recording.application}</span>
              <span className="recording-card-id mono">Catalog ID · {recording.id}</span>
              <div className="recording-card-metrics">
                <span>
                  {recording.playableSegmentCount}/{recording.segmentCount} playable
                </span>
                <span>{formatBytes(recording.playableByteLength)}</span>
                <span>{formatDate(recording.startedAt)}</span>
              </div>
            </button>
          ))}
          {recordings.length === 0 && <EmptyState>No recordings match this view.</EmptyState>}
        </div>
      </section>

      <section className="panel recording-player-panel">
        {selected ? (
          <>
            <header className="recording-player-header">
              <div>
                <span>Rerun recording · {selected.application}</span>
                <h2>{selected.recordingKey}</h2>
                <button
                  className="recording-uri"
                  onClick={() => void copyRecordingUri()}
                  title="Copy canonical recording URI"
                >
                  <span className="mono">recording://recordings/{selected.id}</span>
                  {copied ? <Check size={13} /> : <Copy size={13} />}
                </button>
              </div>
              <StatusPill value={selected.state} />
            </header>
            <div className="recording-facts">
              <div><span>Lifecycle</span><strong>{lifecycleDetail[selected.state]}</strong></div>
              <div>
                <span>Playable segments</span>
                <strong>{selected.playableSegmentCount} of {selected.segmentCount}</strong>
              </div>
              <div><span>Playable size</span><strong>{formatBytes(selected.playableByteLength)}</strong></div>
              <div><span>Started</span><strong>{formatDate(selected.startedAt)}</strong></div>
              <div>
                <span>Ended</span>
                <strong>{selected.state === "live" ? "Live now" : selected.endedAt ? formatDate(selected.endedAt) : "Not reported"}</strong>
              </div>
            </div>
            <div className="recording-viewer">
              {loading ? (
                <div className="recording-viewer-state"><div className="loading-mark" /><span>Authorizing recording segments…</span></div>
              ) : playbackError ? (
                <div className="recording-viewer-state recording-viewer-error">
                  <strong>Playback unavailable</strong>
                  <span>{playbackError}</span>
                  <button className="button button-secondary" onClick={reloadPlayback}>
                    <RefreshCw size={14} /> Retry
                  </button>
                </div>
              ) : !playbackSource ? (
                selected.state === "live" ? (
                  <div className="recording-viewer-state">
                    <FileStack size={30} />
                    <strong>Live capture is starting.</strong>
                    <span>
                      This producer is connected. Rerun will follow its active segment as soon as
                      Recording Hub opens it.
                    </span>
                  </div>
                ) : selected.playableSegmentCount > 0 ? (
                  <div className="recording-viewer-state recording-viewer-error">
                    <strong>Playback manifest is inconsistent.</strong>
                    <span>
                      The catalog reports {selected.playableSegmentCount} playable segment
                      {selected.playableSegmentCount === 1 ? "" : "s"}, but the playback manifest
                      returned none.
                    </span>
                    <button className="button button-secondary" onClick={reloadPlayback}>
                      <RefreshCw size={14} /> Reload manifest
                    </button>
                  </div>
                ) : (
                  <div className="recording-viewer-state">
                    <FileStack size={30} />
                    <strong>This recording has no playable data.</strong>
                    <span>
                      Its lifecycle is {selected.state}; no frozen or sealed RRD segment is
                      available.
                    </span>
                  </div>
                )
              ) : (
                <ViewerBoundary recordingId={selected.id}>
                  <Suspense fallback={<div className="recording-viewer-state"><div className="loading-mark" /><span>Loading Rerun 0.34.1…</span></div>}>
                    <GovernedRerunViewer
                      key={`${selected.id}:${playbackSource.mode}:${playbackSource.url}`}
                      recordingId={selected.id}
                      source={playbackSource}
                    />
                  </Suspense>
                </ViewerBoundary>
              )}
            </div>
            {manifest && (
              <footer className="recording-player-footer">
                <Play size={14} />
                <span>
                  {manifest.live_segment
                    ? `Live · following active segment ${manifest.live_segment.ordinal + 1}`
                    : `${manifest.segments.length} authorized RRD segment${manifest.segments.length === 1 ? "" : "s"} normalized into one replay timeline.`}
                </span>
              </footer>
            )}
          </>
        ) : (
          <div className="recording-empty-player">
            <FileStack size={34} />
            <h2>Select a recording</h2>
            <p>Inspect lifecycle details and open its governed RRD segments in the embedded Rerun viewer.</p>
          </div>
        )}
      </section>
    </div>
  );
}
