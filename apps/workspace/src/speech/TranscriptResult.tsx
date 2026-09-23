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
      if (!response.ok) throw new Error("You don't have access to this transcript file.");
      // HTTP compression may omit Content-Length or report encoded bytes.
      // Bound the decoded stream by the immutable Artifact's actual byte length.
      const length = output.transcript.byte_len;
      if (!Number.isSafeInteger(length) || length <= 0 || length > 4 * 1024 * 1024) throw new Error("This transcript is too large to preview (over 4 MiB). Use Download transcript.");
      const document = parseSpeech("TranscriptDocument", await boundedJson(response, length, length));
      if (document.schema !== "veoveo.speech-transcript/v1" || artifactId(document.source_artifact_uri) !== source) throw new Error("This transcript doesn't match its recording, so it can't be shown.");
      if (!controller.signal.aborted) setDocument(document);
    })().catch(error => { if (!controller.signal.aborted) setError(error instanceof Error ? error.message : "The transcript couldn't be opened. Try Download transcript."); });
    return () => controller.abort();
  }, [open, transcript, source, output]);
  if (!output || !source || !transcript || !captions) return <p className="error">This transcription result couldn't be read.</p>;
  return <section className="speech-result" aria-label="Recording transcript">
    <button onClick={() => setOpen(value => !value)}>{open ? "Close transcript" : "Open transcript"}</button>
    <a href={artifactPath(transcript, "download")}>Download transcript</a><a href={artifactPath(captions, "download")}>Download captions</a>
    {open && <>
      <p className="muted">Timestamps are approximate, and speakers are not identified.</p>
      <audio ref={audio} controls preload="metadata" src={artifactPath(source, "download")} onError={() => setError("The recording can't be played here. You may not have access to it, or this browser can't play its format.")}/>
      {document ? <div className="speech-segments">{document.transcript.segments.map((segment, index) => <p key={index}><button aria-label={`Play from ${timestamp(segment.start)}`} onClick={() => {
        const player = audio.current;
        if (player) { player.currentTime = segment.start; void player.play().catch(() => setError("Playback couldn't start. Use the player controls above.")); }
      }}>{timestamp(segment.start)}</button> {segment.text}</p>)}</div> : !error && <p role="status">Opening transcript…</p>}
      {error && <p role="alert" className="error">{error}</p>}
      <details><summary>Share transcript link</summary><p>People you share this link with still need access to the file.</p><code>{output.transcript.artifact_uri}</code><button onClick={() => void navigator.clipboard.writeText(output.transcript.artifact_uri).catch(() => setError("The link couldn't be copied. Select it and copy it manually."))}>Copy transcript link</button></details>
    </>}
  </section>;
}
