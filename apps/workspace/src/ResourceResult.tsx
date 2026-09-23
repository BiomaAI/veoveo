import { useEffect, useRef, useState } from "react";
import { Download, Eye, X } from "lucide-react";
import { artifactId, artifactPath, canPreviewImage } from "./resources.ts";
import type { OperationResource } from "./generated/workspace.ts";

export function ResourceResult({ resource }: { resource: OperationResource }) {
  const id = artifactId(resource.uri);
  const [preview, setPreview] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string>();
  const active = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => active.current?.abort(), []);
  async function open() {
    if (!id || busy) return;
    const controller = new AbortController(); active.current = controller;
    setBusy(true); setNotice(undefined);
    try {
      const response = await fetch(artifactPath(id, "preview"), { method: "HEAD", credentials: "same-origin", cache: "no-store", redirect: "error",
        signal: AbortSignal.any([controller.signal, AbortSignal.timeout(10_000)]) });
      if (response.status === 401) { window.dispatchEvent(new Event("workspace-auth-expired")); return; }
      if ([403, 404].includes(response.status)) throw new Error("You don't have access to this file. Ask its owner to share it with you.");
      if (!response.ok) throw new Error("This file couldn't be opened. Try again in a moment.");
      if (!canPreviewImage(response.headers.get("content-type"), response.headers.get("content-length"))) {
        setNotice("Preview works only for images up to 20 MiB. Use Download to open this file."); return;
      }
      setPreview(true);
    } catch (error) {
      if (!controller.signal.aborted) setNotice(error instanceof Error ? error.message : "The preview couldn't be opened. Try Download instead.");
    } finally { if (!controller.signal.aborted) setBusy(false); }
  }
  return <div className="task-resource"><strong>{resource.name}</strong>
    {id && <div className="resource-actions"><button disabled={busy} onClick={() => preview ? setPreview(false) : void open()}>
      {preview ? <X size={14}/> : <Eye size={14}/>} {busy ? "Checking access…" : preview ? "Close preview" : "Preview"}</button>
      <a href={artifactPath(id, "download")} className="resource-download"><Download size={14}/> Download</a></div>}
    {preview && id && <img className="task-image" src={artifactPath(id, "preview")} alt={resource.name} onError={() => { setPreview(false); setNotice("The image couldn't be loaded. You may not have access to it."); }}/>}
    {notice && <p className="muted" role="status">{notice}</p>}
    <details><summary>File link</summary><code>{resource.uri}</code><button onClick={() => void navigator.clipboard.writeText(resource.uri).catch(() => setNotice("The link couldn't be copied. Select it and copy it manually."))}>Copy file link</button></details>
  </div>;
}
