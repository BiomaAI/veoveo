import { useState } from "react";
import { createPortal } from "react-dom";
import { UploadPanel } from "../../console/web/src/uploads/UploadPanel.tsx";
import type { UploadQueue, QueueState } from "../../console/web/src/uploads/queue.ts";
import type { Receipt } from "../../console/web/src/uploads/model.ts";
import { ResourceResult } from "./ResourceResult.tsx";
import "./uploads.css";

export function Uploads({ queue, state, close }: { queue: UploadQueue; state: QueueState; close: () => void }) {
  const [preview, setPreview] = useState<Receipt>();
  return createPortal(<>
    {!preview && <UploadPanel queue={queue} state={state} onClose={close} backgroundSelector=".workspace" onView={async receipt => setPreview(receipt)}/>}
    {preview && <dialog className="upload-result-modal" ref={node => { if (node && !node.open) node.showModal(); }} onCancel={() => setPreview(undefined)}>
      <button onClick={() => setPreview(undefined)}>Close file details</button>
      <h2>{preview.filename}</h2><p>Access follows this file's policy. Adding someone to a chat does not share this file.</p>
      <ResourceResult resource={{ uri: preview.artifact_uri, name: preview.filename, mimeType: preview.mime_type }}/>
    </dialog>}
  </>, document.body);
}
