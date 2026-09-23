import { useEffect, useMemo, useRef, useState } from "react";
import { parseSpeech } from "./api.ts";
import { artifactId, artifactPath } from "../resources.ts";
import type { TranscriptDocument } from "../generated/speech.ts";
import { boundedJson } from "../../../console/web/src/browserHttp.ts";

function timestamp(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${Math.floor(seconds % 60).toString().padStart(2, "0")}`;
}
export function TranscriptResult({ value }: { value: unknown }) {
  const output = useMemo(() => { try { return parseSpeech("TranscriptionOutput", value); } catch { return undefined; } }, [value]);
  const [document, setDocument] = useState<TranscriptDocument>();
  const [error, setError] = useState("");
  const [open, setOpen] = useState(false);
  const audio = useRef<HTMLAudioElement>(null);
  const source = output && artifactId(output.source_artifact_uri);
  const transcript = output && artifactId(output.transcript.artifact_uri);
  const captions = output && artifactId(output.captions.artifact_uri);
  useEffect(() => {
    if (!open || !transcript || !output) return;
    const controller = new AbortController(); setError(""); setDocument(undefined);
    void (async () => {
      const response = await fetch(artifactPath(transcript, "download"), { credentials: "same-origin", cache: "no-store", redirect: "error", signal: AbortSignal.any([controller.signal, AbortSignal.timeout(15_000)]) });
      if (response.status === 401) window.dispatchEvent(new Event("workspace-auth-expired"));
      if (!response.ok) throw new Error("This transcript is unavailable with your current file access.");
      // HTTP compression may omit Content-Length or report encoded bytes.
      // Bound the decoded stream by the immutable Artifact's actual byte length.
      const length = output.transcript.byte_len;
      if (!Number.isSafeInteger(length) || length <= 0 || length > 4 * 1024 * 1024) throw new Error("The transcript exceeds the preview limit.");
      const document = parseSpeech("TranscriptDocument", await boundedJson(response, length, length));
      if (document.schema !== "veoveo.speech-transcript/v1" || artifactId(document.source_artifact_uri) !== source) throw new Error("The transcript source could not be verified.");
      if (!controller.signal.aborted) setDocument(document);
    })().catch(error => { if (!controller.signal.aborted) setError(error instanceof Error ? error.message : "The transcript could not be opened."); });
    return () => controller.abort();
  }, [open, transcript, source, output]);
  if (!output || !source || !transcript || !captions) return <p className="error">The transcript result could not be verified.</p>;
  return <section className="speech-result" aria-label="Recording transcript">
    <button onClick={() => setOpen(value => !value)}>{open ? "Close transcript" : "Open transcript"}</button>
    <a href={artifactPath(transcript, "download")}>Download transcript</a><a href={artifactPath(captions, "download")}>Download captions</a>
    {open && <>
      <p className="muted">Playback and downloads require current file access. Timestamps are approximate; speaker identities are not assigned.</p>
      <audio ref={audio} controls preload="metadata" src={artifactPath(source, "download")} onError={() => setError("Source playback is unavailable with current access or this browser’s audio support.")}/>
      {document ? <div className="speech-segments">{document.transcript.segments.map((segment, index) => <p key={index}><button aria-label={`Play from ${timestamp(segment.start)}`} onClick={() => {
        const player = audio.current;
        if (player) { player.currentTime = segment.start; void player.play().catch(() => setError("Playback could not start. Open the recording with an audio player.")); }
      }}>{timestamp(segment.start)}</button> {segment.text}</p>)}</div> : !error && <p role="status">Opening transcript…</p>}
      {error && <p role="alert" className="error">{error}</p>}
      <details><summary>Share transcript link</summary><p>Sharing a link does not grant file access.</p><code>{output.transcript.artifact_uri}</code><button onClick={() => void navigator.clipboard.writeText(output.transcript.artifact_uri).catch(() => setError("The link could not be copied."))}>Copy transcript link</button></details>
    </>}
  </section>;
}
