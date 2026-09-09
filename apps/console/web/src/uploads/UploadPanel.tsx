import { useEffect, useRef, useState } from "react";
import { Upload, X } from "lucide-react";
import { formatBytes } from "../format";
import { redirectToLogin } from "../auth";
import type { Entry, Receipt } from "./model";
import type { QueueState, UploadQueue } from "./queue";
import "./uploads.css";

export function UploadPanel({ queue, state, onClose, onView }: {
  queue: UploadQueue; state: QueueState; onClose: () => void; onView: (receipt: Receipt) => Promise<void>;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  useEffect(() => { close.current = onClose; }, [onClose]);
  const [dropActive, setDropActive] = useState(false);
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    const dialog = panel.current;
    dialog?.querySelector<HTMLElement>("button")?.focus();
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); close.current(); }
      if (event.key !== "Tab" || !dialog) return;
      const focusable = [...dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), a[href], [tabindex="0"]')];
      const first = focusable[0], last = focusable.at(-1);
      if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) { event.preventDefault(); first?.focus(); }
    };
    document.addEventListener("keydown", keydown);
    const shell = document.querySelector<HTMLElement>(".app-shell");
    // The panel is a sibling of the application shell, so underlying controls are inert.
    shell?.setAttribute("inert", "");
    return () => { document.removeEventListener("keydown", keydown); shell?.removeAttribute("inert"); previous?.focus(); };
  }, []);
  const selected = state.entries.filter((entry) => entry.phase === "Selected");
  const bytes = selected.reduce((sum, entry) => sum + entry.descriptor.byte_len, 0);
  const phases = state.entries.map((entry) => `${entry.descriptor.filename}: ${entry.phase}`).join(". ");
  return <div className="drawer-layer upload-layer">
    <button className="drawer-scrim" aria-label="Close uploads; transfers continue" onClick={onClose} />
    <div ref={panel} className="drawer upload-panel" role="dialog" aria-modal="true" aria-labelledby="upload-title" aria-describedby="upload-access">
      <header><div><span>Artifacts</span><h2 id="upload-title">Upload to {state.policy?.destination_name ?? "your Work Context"}</h2></div>
        <button className="icon-button" onClick={onClose} aria-label="Close uploads; transfers continue"><X size={18} /></button></header>
      <div className="upload-content">
        <p id="upload-access">{state.policy?.access_description ?? "Loading destination and access…"}</p>
        {state.policyError && <p role="alert">{state.policyError} <button className="button button-secondary" onClick={() => void queue.refreshPolicy()}>Retry policy</button></p>}
        {state.policy && !state.policy.allowed && <p role="status">{state.policy.explanation} <button className="button button-secondary" onClick={() => void queue.refreshPolicy()}>Check access again</button></p>}
        {state.policy?.allowed && <>
          <div className={`upload-drop ${dropActive ? "upload-drop-active" : ""}`}
            onDragOver={(event) => { event.preventDefault(); setDropActive(true); }}
            onDragLeave={() => setDropActive(false)}
            onDrop={(event) => { event.preventDefault(); setDropActive(false); queue.select([...event.dataTransfer.files]); }}>
            <Upload size={24} aria-hidden="true" />
            <label htmlFor="upload-files">Choose files or drop them here</label>
            <input id="upload-files" type="file" multiple onChange={(event) => { queue.select([...(event.target.files ?? [])]); event.target.value = ""; }} />
            <p>Review your selection before starting. Each file creates a new artifact.</p>
          </div>
          <p className="subdued">Up to {formatBytes(state.policy.policy?.max_object_bytes ?? 0)} per file.
            {state.policy.available_bytes !== undefined && ` ${formatBytes(state.policy.available_bytes)} available when last checked.`}</p>
        </>}
        {state.notice && <p role="status">{state.notice}</p>}
        {state.persistenceError && <p role="alert">{state.persistenceError}</p>}
        {selected.length > 0 && <div className="upload-start">
          <span>{selected.length} selected · {formatBytes(bytes)}</span>
          <button className="button button-primary" onClick={() => queue.start()} disabled={!state.policy?.allowed}>Upload {selected.length} {selected.length === 1 ? "file" : "files"}</button>
        </div>}
        {state.policy?.available_bytes !== undefined && bytes > state.policy.available_bytes && <p role="status">The selected files exceed the last checked storage allowance. Some files may need to wait until space is available.</p>}
        <div className="upload-queue-heading"><h3>Uploads</h3>
          {state.entries.some((entry) => ["Ready", "Cancelled"].includes(entry.phase)) && <button className="button button-secondary" onClick={() => queue.clearCompleted()}>Clear finished</button>}
        </div>
        <p className="visually-hidden" aria-live="polite" aria-atomic="true">{phases}</p>
        {!state.entries.length && <p className="subdued">Select files to start an upload.</p>}
        <ul className="upload-queue">{state.entries.map((entry) => <UploadRow key={entry.key} entry={entry} queue={queue} onView={onView} />)}</ul>
      </div>
      <footer>Keep this tab open while sending files. Closing the panel or navigating within Console keeps uploads running.</footer>
    </div>
  </div>;
}

function UploadRow({ entry, queue, onView }: { entry: Entry; queue: UploadQueue; onView: (receipt: Receipt) => Promise<void> }) {
  const [actionMessage, setActionMessage] = useState<string>();
  const [viewing, setViewing] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const active = ["Queued", "Preparing", "Uploading", "Checking file"].includes(entry.phase);
  const resume = !entry.restartRequired && ["Paused", "Waiting for connection", "Needs attention", "Select file"].includes(entry.phase);
  const terminal = ["Ready", "Cancelled"].includes(entry.phase);
  const selectAgain = !entry.restartRequired && !entry.file && !terminal && entry.phase !== "Finishing upload" && entry.phase !== "Cancelling";
  const view = async () => {
    if (!entry.receipt) return;
    setViewing(true); setActionMessage(undefined);
    try { await onView(entry.receipt); } catch (error) { setActionMessage(error instanceof Error ? error.message : "Artifact details are not available yet. Try again."); }
    finally { setViewing(false); }
  };
  return <li className="upload-entry" data-upload-id={entry.uploadId} data-upload-phase={entry.phase}>
    <div className="upload-entry-title"><strong title={entry.descriptor.filename}>{entry.descriptor.filename}</strong><span>{entry.phase}</span></div>
    <p className="subdued">{formatBytes(entry.descriptor.byte_len)} · {entry.descriptor.mime_type}</p>
    {entry.uploadId && !terminal && <>
      <progress max={entry.descriptor.byte_len || 1} value={entry.phase === "Finishing upload" && entry.descriptor.byte_len === 0 ? 1 : entry.sent} aria-label={`Bytes sent for ${entry.descriptor.filename}`} />
      <p>{formatBytes(entry.sent)} sent · {formatBytes(entry.accepted)} accepted
        {entry.speed !== undefined && ` · ${formatBytes(entry.speed)}/s`}
        {entry.eta !== undefined && entry.eta > 0 && ` · about ${entry.eta < 60 ? `${entry.eta} sec` : `${Math.ceil(entry.eta / 60)} min`} remaining to send`}</p>
    </>}
    {entry.phase === "Finishing upload" && <p>All bytes accepted. Verifying the file and publishing your artifact…</p>}
    {entry.message && <p className="upload-message">{entry.message}</p>}
    {actionMessage && <p role="status">{actionMessage}</p>}
    <div className="upload-entry-actions">
      {entry.phase === "Selected" && <button className="button button-secondary" aria-label={`Remove ${entry.descriptor.filename}`} onClick={() => queue.remove(entry.key)}>Remove</button>}
      {active && <button className="button button-secondary" aria-label={`Pause ${entry.descriptor.filename}`} onClick={() => queue.pause(entry.key)}>Pause</button>}
      {resume && <button className="button button-secondary" aria-label={`Resume ${entry.descriptor.filename}`} onClick={() => queue.resume(entry.key)}>{entry.phase === "Needs attention" ? "Retry" : "Resume"}</button>}
      {entry.restartRequired && <button className="button button-primary" aria-label={`Start again ${entry.descriptor.filename}`} onClick={() => void queue.restart(entry.key)}>Start again</button>}
      {selectAgain && <><button className="button button-secondary" aria-label={`Select original file for ${entry.descriptor.filename}`} onClick={() => fileInput.current?.click()}>Select file</button>
        <input className="visually-hidden" tabIndex={-1} ref={fileInput} type="file" aria-label={`Original file for ${entry.descriptor.filename}`} onChange={(event) => { const file = event.target.files?.[0]; if (file) queue.reselect(entry.key, file); event.target.value = ""; }} /></>}
      {entry.phase === "Sign in to continue" && <button className="button button-primary" aria-label={`Sign in to continue ${entry.descriptor.filename}`} onClick={() => { queue.pauseAll(true); redirectToLogin(); }}>Sign in</button>}
      {!terminal && entry.phase !== "Selected" && <button className="button button-secondary" aria-label={`Cancel ${entry.descriptor.filename}`} onClick={() => void queue.cancel(entry.key)}>{entry.phase === "Cancelling" ? "Retry cancellation" : "Cancel"}</button>}
      {entry.file && !entry.cancelRequested && <button className="button button-secondary" aria-label={`Upload another copy of ${entry.descriptor.filename}`} onClick={() => queue.select([entry.file!], true)}>Upload another copy</button>}
      {entry.receipt && <>
        <button className="button button-primary" aria-label={`View artifact ${entry.descriptor.filename}`} onClick={() => void view()} disabled={viewing}>{viewing ? "Opening…" : "View artifact"}</button>
        <button className="button button-secondary" aria-label={`Copy artifact URI for ${entry.descriptor.filename}`} onClick={() => { void navigator.clipboard.writeText(entry.receipt!.artifact_uri).then(() => setActionMessage("Artifact URI copied."), () => setActionMessage("Copy failed. Open the artifact to copy its URI.")); }}>Copy URI</button>
      </>}
    </div>
  </li>;
}
