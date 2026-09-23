import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { FileAudio, Upload } from "lucide-react";
import { api, ApiError } from "../api.ts";
import { artifactId } from "../resources.ts";
import type { QueueState } from "../../../console/web/src/uploads/queue.ts";

export function Recording({ chat, uploads, disabled, onUpload }: { chat: string; uploads: QueueState; disabled: boolean; onUpload: () => void }) {
  const [open, setOpen] = useState(false);
  const [uri, setUri] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [accepted, setAccepted] = useState(false);
  const attempt = useRef<{ id: string; uri: string } | undefined>(undefined);
  const queryClient = useQueryClient();
  const files = uploads.entries.filter(entry => entry.phase === "Ready" && entry.receipt && /^(audio|video)\//.test(entry.receipt.mime_type));
  async function start() {
    const id = artifactId(uri.trim());
    if (!id || busy || disabled) return;
    const canonical = `artifact://${id}`;
    if (attempt.current && attempt.current.uri !== canonical) { setNotice("Retry the original recording first so Veoveo can check whether its transcription started."); return; }
    const request = attempt.current ?? { id: crypto.randomUUID(), uri: canonical };
    attempt.current = request; setBusy(true); setNotice(""); setAccepted(false);
    try {
      await api.startOperation(chat, { id: request.id, tool: "speech__transcribe", arguments: { artifact_uri: canonical } });
      attempt.current = undefined;
      setAccepted(true); setNotice("Transcription started. Follow it in your Activity; you can leave this chat while it runs.");
      await queryClient.invalidateQueries({ queryKey: ["operations"] });
    } catch (error) {
      if (error instanceof ApiError && [400, 401, 403, 404, 422].includes(error.status)) attempt.current = undefined;
      setNotice(attempt.current ? "Veoveo couldn't tell whether transcription started. Select Retry transcription; it won't start twice."
        : error instanceof Error ? error.message : "Veoveo couldn't start transcription. Try again.");
    }
    finally { setBusy(false); }
  }
  return <div className="recording-transcription">
    <button type="button" disabled={disabled} onClick={() => setOpen(value => !value)}><FileAudio size={15}/> Transcribe a recording</button>
    {open && <div className="recording-picker">
      <p>Choose an uploaded audio or video file, up to 2 hours and 2 GiB. The task appears only in your Activity. The transcript file gets the same access rules as other files in this Work Context.</p>
      <button type="button" disabled={disabled} onClick={onUpload}><Upload size={14}/> Upload recording</button>
      {!!files.length && <label>Completed uploads<select aria-label="Recording to transcribe" value={uri} disabled={busy || !!attempt.current} onChange={event => { setUri(event.target.value); setAccepted(false); }}><option value="">Choose a recording</option>{files.map(entry => <option key={entry.key} value={entry.receipt!.artifact_uri}>{entry.receipt!.filename}</option>)}</select></label>}
      <label>Recording file link<input value={uri} readOnly={busy || !!attempt.current} placeholder="artifact://…" onChange={event => { setUri(event.target.value); setAccepted(false); }}/></label>
      <button type="button" disabled={disabled || busy || !artifactId(uri.trim()) || accepted} onClick={() => void start()}>{busy ? "Starting transcription…" : attempt.current ? "Retry transcription" : "Transcribe"}</button>
      {notice && <p role="status">{notice} {accepted && <a href={"/workspace/?view=activity"}>Open Activity</a>}</p>}
    </div>}
  </div>;
}
