import { useId, useState } from "react";
import { createPortal } from "react-dom";
import { Paperclip, Upload, X } from "lucide-react";
import type { QueueState } from "../../console/web/src/uploads/queue.ts";
import type { ChatAttachment } from "./generated/workspace.ts";
import { attachmentReference } from "./attachments.ts";
import { ResourceResult } from "./ResourceResult.tsx";

export function MessageAttachments({ attachments }: { attachments: ChatAttachment[] }) {
  if (!attachments.length) return null;
  return <section className="message-attachments" aria-label="Attachments">
    {attachments.map(file => <ResourceResult key={file.id} resource={{ uri: `artifact://${file.id}`, name: file.name }}/>) }
  </section>;
}

export function AttachmentComposer({ value, onChange, disabled, uploads, onUpload }: {
  value: ChatAttachment[]; onChange: (value: ChatAttachment[]) => void; disabled: boolean;
  uploads: QueueState; onUpload: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [uri, setUri] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string>();
  const title = useId();
  const ready = uploads.entries.filter(entry => entry.phase === "Ready" && entry.receipt);
  function add(link: string, label: string) {
    if (disabled) return;
    try {
      const file = attachmentReference(link, label);
      if (value.length >= 8) throw new Error("A message can include up to eight files.");
      if (value.some(item => item.id === file.id)) throw new Error("This file is already attached.");
      onChange([...value, file]); setOpen(false); setUri(""); setName(""); setError(undefined);
    } catch (error) { setError(error instanceof Error ? error.message : "This file couldn't be added. Check the link and try again."); }
  }
  return <div className="attachment-composer">
    <button className="attach-files" disabled={disabled || value.length >= 8} onClick={() => { setError(undefined); setOpen(true); }}><Paperclip size={15}/> Attach files</button>
    {!!value.length && <>
      <ul aria-label="Files to send">{value.map(file => <li key={file.id}><Paperclip size={14}/><span>{file.name}</span><button className="icon-button" aria-label={`Remove attachment ${file.name}`} disabled={disabled} onClick={() => onChange(value.filter(item => item.id !== file.id))}><X size={14}/></button></li>)}</ul>
      <p className="composer-note">Everyone in this chat will see these file names. Each person still needs their own access to open a file, and agents get the links, not the contents.</p>
    </>}
    {open && createPortal(<dialog className="modal-backdrop" ref={node => { if (node && !node.open) node.showModal(); }} aria-labelledby={title} onCancel={() => setOpen(false)}>
      <div className="modal attachment-picker"><div className="details-heading"><h2 id={title}>Attach files</h2><button className="icon-button" aria-label="Close attachment picker" onClick={() => setOpen(false)}><X size={18}/></button></div>
        <p>Choose files to include in your next message.</p>
        <button disabled={disabled} onClick={() => { setOpen(false); onUpload(); }}><Upload size={15}/> Upload a file</button>
        {!!ready.length && <section aria-label="Completed uploads"><h3>Completed uploads</h3><div className="attachment-choices">{ready.map(entry => <button key={entry.key} disabled={disabled || value.some(file => file.id === entry.receipt!.artifact_id)} onClick={() => add(entry.receipt!.artifact_uri, entry.receipt!.filename)}><Paperclip size={15}/>{entry.receipt!.filename}</button>)}</div></section>}
        <form onSubmit={event => { event.preventDefault(); add(uri, name); }}>
          <h3>Use a file link</h3><label>File link<input value={uri} maxLength={200} onChange={event => setUri(event.target.value)} placeholder="artifact://…" autoComplete="off"/></label>
          <label>Name shown in chat<input value={name} maxLength={255} onChange={event => setName(event.target.value)} placeholder="e.g. Project notes" autoComplete="off"/></label>
          {error && <p role="alert" className="error">{error}</p>}
          <button className="primary" disabled={disabled || !uri.trim() || !name.trim()}>Add file link</button>
        </form>
      </div>
    </dialog>, document.body)}
  </div>;
}
