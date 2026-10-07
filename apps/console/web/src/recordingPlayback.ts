import { ChronoTimestamp } from "./chronoTimestamp";
import schema from "./generated/recording-playback.schema.json";
import { compileGeneratedSchema } from "./jsonSchema";
import type { RecordingPlaybackManifest } from "./types";

const validate = compileGeneratedSchema(schema);

/** Admit current owned JSON and visible source relationships before a viewer starts. */
export function recordingPlaybackValue(
  value: unknown,
  selectedRecording: string,
): RecordingPlaybackManifest {
  if (!validate.safeParse(value).success) throw new Error("Invalid Recording playback manifest");
  const manifest = value as RecordingPlaybackManifest;
  const archive = manifest.archive ?? null;
  if (archive !== null) {
    // Recording owns this selected Redap route and UUID-to-TUID codec. The URL
    // implementation parses components; the renderer still consumes the original URI.
    const address = new URL(archive.uri);
    // A fixed URL carrier applies the maintained network-host codec without
    // reconstructing the supplied external origin or the playback address.
    const networkHost = new URL("https://host.invalid");
    networkHost.hostname = address.hostname;
    const loopback = address.hostname === "localhost" || address.hostname === "[::1]" ||
      /^127(?:\.\d{1,3}){3}$/.test(address.hostname);
    const query = new URLSearchParams([["segment_id", selectedRecording]]);
    const dataset = manifest.datasetId.replaceAll("-", "");
    const entry = dataset.slice(0, 16).toUpperCase() + dataset.slice(16);
    if (
      !["rerun:", "rerun+http:"].includes(address.protocol) || !address.hostname ||
      !address.port || Number(address.port) <= 0 || address.username || address.password || address.hash ||
      networkHost.hostname !== address.hostname || ["0.0.0.0", "[::]"].includes(address.hostname) ||
      (loopback && ((address.protocol === "rerun:" && address.port === "443") ||
        (address.protocol === "rerun+http:" && address.port === "80"))) ||
      address.href !== archive.uri || address.pathname !== `/dataset/${entry}` ||
      address.search !== `?${query.toString()}` ||
      [...address.searchParams].length !== 1 ||
      address.searchParams.get("segment_id") !== selectedRecording
    ) throw new Error("Recording archive address differs from its source");
  }

  const nonblank = (text: string) => /\P{White_Space}/u.test(text);
  const text = (value: string, maximum: number) => nonblank(value) &&
    new TextEncoder().encode(value).byteLength <= maximum && !/\p{Cc}/u.test(value);
  const started = ChronoTimestamp.parse(manifest.startedAt);
  const ended = manifest.endedAt == null ? undefined : ChronoTimestamp.parse(manifest.endedAt);
  ChronoTimestamp.parse(manifest.access.expiresAt);
  if (
    !text(manifest.applicationId, 512) || !text(manifest.recordingKey, 512) ||
    !text(manifest.catalogRevision, 128) || !nonblank(manifest.access.redapToken) ||
    (archive !== null && (!text(archive.rrdVersion, 128) || !text(archive.optimizationProfile, 128) ||
      archive.byteLen <= 0 || archive.layerCount <= 0)) ||
    (manifest.live != null && (manifest.live.historySeconds <= 0 || manifest.live.videoPrerollSeconds <= 0)) ||
    (manifest.blueprint != null && !text(manifest.blueprint.blueprintId, 512)) ||
    manifest.recordingSegmentId !== selectedRecording ||
    (ended !== undefined && ended.compare(started) < 0) ||
    (manifest.state === "live"
      ? manifest.live == null || archive !== null || manifest.endedAt != null
      : manifest.live != null) ||
    (archive !== null && (
      archive.datasetId !== manifest.datasetId ||
      archive.recordingSegmentId !== manifest.recordingSegmentId ||
      archive.catalogRevision !== manifest.catalogRevision
    ))
  ) {
    throw new Error("Recording playback manifest differs from its source");
  }
  return manifest;
}
