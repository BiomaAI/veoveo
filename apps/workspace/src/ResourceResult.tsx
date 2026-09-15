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
      if ([403, 404].includes(response.status)) throw new Error("This file is unavailable with your current access. A chat or Task link does not grant file access.");
      if (!response.ok) throw new Error("The file could not be opened. Try again shortly.");
      if (!canPreviewImage(response.headers.get("content-type"), response.headers.get("content-length"))) {
        setNotice("This file is available to download. Inline preview supports raster images up to 20 MiB."); return;
      }
      setPreview(true);
    } catch (error) {
      if (!controller.signal.aborted) setNotice(error instanceof Error ? error.message : "The preview could not be opened.");
    } finally { if (!controller.signal.aborted) setBusy(false); }
  }
  return <div className="task-resource"><strong>{resource.name}</strong>
    {id && <div className="resource-actions"><button disabled={busy} onClick={() => preview ? setPreview(false) : void open()}>
      {preview ? <X size={14}/> : <Eye size={14}/>} {busy ? "Checking access…" : preview ? "Close preview" : "Preview"}</button>
      <a href={artifactPath(id, "download")} className="resource-download"><Download size={14}/> Download</a></div>}
    {preview && id && <img className="task-image" src={artifactPath(id, "preview")} alt={resource.name} onError={() => { setPreview(false); setNotice("The image could not be loaded with your current access."); }}/>}
    {notice && <p className="muted" role="status">{notice}</p>}
    <details><summary>Resource link</summary><code>{resource.uri}</code><button onClick={() => void navigator.clipboard.writeText(resource.uri).catch(() => setNotice("The resource link could not be copied."))}>Copy resource link</button></details>
  </div>;
}
