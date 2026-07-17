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
import { FileStack, Play, Search } from "lucide-react";
import { loadRecordingPlayback, recordingSegmentUrl } from "../api";
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
  const [selectedId, setSelectedId] = useState<string | undefined>(initialRecordingId);
  const [query, setQuery] = useState("");
  const [stateFilter, setStateFilter] = useState<RecordingStateFilter>("all");
  const [manifest, setManifest] = useState<RecordingPlaybackManifest>();
  const [loading, setLoading] = useState(Boolean(initialRecordingId));
  const [playbackError, setPlaybackError] = useState<string>();

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

  const selected = snapshot.recordings.find((recording) => recording.id === selectedId);

  useEffect(() => {
    if (!selectedId) return;
    const controller = new AbortController();
    void loadRecordingPlayback(selectedId, controller.signal)
      .then(setManifest)
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
  }, [selected?.byteLength, selected?.segments, selected?.state, selectedId]);

  const selectRecording = (recordingId: string) => {
    if (recordingId === selectedId) return;
    setManifest(undefined);
    setPlaybackError(undefined);
    setLoading(true);
    setSelectedId(recordingId);
    onRecordingSelect(recordingId);
  };

  const playbackSegments = useMemo(
    () =>
      manifest?.segments
        .slice()
        .sort((left, right) => left.ordinal - right.ordinal)
        .map((segment) => ({
          ordinal: segment.ordinal,
          byteLength: segment.byte_len,
          url: recordingSegmentUrl(manifest.recording_id, segment.segment_id),
        })) ?? [],
    [manifest]
  );

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
              className={recording.id === selectedId ? "recording-card recording-card-active" : "recording-card"}
              onClick={() => selectRecording(recording.id)}
              aria-pressed={recording.id === selectedId}
            >
              <div className="recording-card-head">
                <strong>{recording.recordingKey}</strong>
                <StatusPill value={recording.state} />
              </div>
              <span>{recording.application}</span>
              <div className="recording-card-metrics">
                <span>{recording.segments} segments</span>
                <span>{formatBytes(recording.byteLength)}</span>
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
                <span>Rerun playback</span>
                <h2>{selected.recordingKey}</h2>
              </div>
              <StatusPill value={selected.state} />
            </header>
            <div className="recording-facts">
              <div><span>Lifecycle</span><strong>{lifecycleDetail[selected.state]}</strong></div>
              <div><span>Segments</span><strong>{selected.segments}</strong></div>
              <div><span>Size</span><strong>{formatBytes(selected.byteLength)}</strong></div>
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
                <div className="recording-viewer-state recording-viewer-error"><strong>Playback unavailable</strong><span>{playbackError}</span></div>
              ) : playbackSegments.length === 0 ? (
                <div className="recording-viewer-state"><FileStack size={30} /><strong>No stable segment is available yet.</strong><span>Live data becomes playable after Recording Hub freezes its first segment.</span></div>
              ) : (
                <ViewerBoundary recordingId={selected.id}>
                  <Suspense fallback={<div className="recording-viewer-state"><div className="loading-mark" /><span>Loading Rerun 0.34.1…</span></div>}>
                    <GovernedRerunViewer
                      key={selected.id}
                      recordingId={selected.id}
                      segments={playbackSegments}
                    />
                  </Suspense>
                </ViewerBoundary>
              )}
            </div>
            {manifest && (
              <footer className="recording-player-footer">
                <Play size={14} />
                <span>{manifest.segments.length} authorized RRD segment{manifest.segments.length === 1 ? "" : "s"} available in ordinal order.</span>
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
