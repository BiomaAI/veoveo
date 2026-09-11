import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { uuidV7 } from "../agentControl";
import { artifactDownloadUrl } from "../api";
import { formatBytes } from "../format";
import type { ComputerSnapshot, ComputerView, FileTransferStage, TransferFileInput } from "../generated/computers";
import type { ArtifactSummary } from "../types";
import type { QueueState } from "../uploads/queue";
import { computerError } from "./api";
import { cancelFileTransfer, readFileTransfer, transferFile } from "./fileApi";
import { artifactId, fileFinished, MAX_FILE_BYTES, readSavedFile, rememberFile, retainedPath, sendSavedFile, type SavedFileTransfer } from "./fileRequest";

const stages: Record<FileTransferStage, string> = {
  queued: "Preparing transfer", dispatched: "Transferring file", containing: "Stopping the interrupted Computer run",
  recovery_required: "Recovery required", completed: "File transferred", failed: "Transfer failed", cancelled: "Transfer cancelled",
};
export function FilesPanel({ computer, scope, snapshot, stale, artifacts, uploads, onUpload }: {
  computer: ComputerView; scope: string; snapshot: ComputerSnapshot; stale: boolean;
  artifacts: ArtifactSummary[]; uploads: QueueState; onUpload: () => void;
}) {
  const cache = useQueryClient();
  const id = computer.computerId;
  const grant = computer.accessMode === "granted" ? computer.grantedAccess.find(g => g.canTransferFiles && g.executionLimits) : undefined;
  const maximumBytes = grant?.executionLimits ? Math.min(MAX_FILE_BYTES, grant.executionLimits.maximumOutputBytes) : MAX_FILE_BYTES;
  const maximumSeconds = grant?.executionLimits ? Math.min(300, grant.executionLimits.maximumSeconds) : 300;
  const key = `veoveo.computers.file:${scope}:${id}`;
  const [saved, setSaved] = useState<{ value?: SavedFileTransfer; error?: string }>(() => {
    try { return { value: readSavedFile(window.sessionStorage, key, id) }; }
    catch { return { error: "The saved transfer cannot be read. Review current work before clearing it." }; }
  });
  const [direction, setDirection] = useState<"import" | "export">("import");
  const [source, setSource] = useState("");
  const [reference, setReference] = useState("");
  const [path, setPath] = useState("");
  const [filename, setFilename] = useState("");
  const [error, setError] = useState<string>();
  const candidates = useMemo(() => {
    const values = new Map<string, { id: string; filename: string; bytes: number | null }>();
    for (const artifact of artifacts) {
      if (artifact.effectiveAccess.read && (artifact.byteLength === null || artifact.byteLength <= maximumBytes))
        values.set(artifact.id, { id: artifact.id, filename: artifact.filename, bytes: artifact.byteLength });
    }
    for (const entry of uploads.entries) {
      if (entry.phase === "Ready" && entry.receipt && entry.receipt.byte_len <= maximumBytes)
        values.set(entry.receipt.artifact_id, { id: entry.receipt.artifact_id, filename: entry.receipt.filename, bytes: entry.receipt.byte_len });
    }
    return [...values.values()];
  }, [artifacts, uploads.entries, maximumBytes]);
  const sourceArtifact = candidates.find(value => value.id === source);
  const activeId = computer.activeExecution?.kind === "file" ? computer.activeExecution.taskId : undefined;
  const taskId = saved.value?.receipt?.taskId ?? activeId;
  const receiptKey = ["computers", "file", id, taskId];
  const receipt = useQuery({ queryKey: receiptKey,
    queryFn: ({ signal }) => readFileTransfer(id, taskId!, signal), enabled: !!taskId && !stale, retry: false,
    // Read Veoveo's durable Task, never provider status. Domain events usually
    // wake this first; this also closes the settlement-to-Task projection gap.
    refetchInterval: query => query.state.error || fileFinished(query.state.data) || query.state.data?.stage === "recovery_required" ? false : 2000,
  });
  useEffect(() => { void cache.invalidateQueries({ queryKey: ["computers", "file", id] }); }, [cache, id, snapshot]);
  const submit = useMutation({ mutationFn: (input: TransferFileInput) => sendSavedFile(window.sessionStorage, key, input, transferFile), retry: false,
    onSuccess: async (result, input) => {
      const queryKey = ["computers", "file", id, result.receipt.taskId];
      await cache.cancelQueries({ queryKey });
      cache.setQueryData(queryKey, result.receipt);
      setSaved({ value: { input, receipt: result.receipt }, error: result.saved ? undefined : "Transfer confirmed. The receipt could not be saved locally; keep this page open until it finishes." });
    },
  });
  const cancel = useMutation({ mutationFn: () => cancelFileTransfer(id, taskId!), retry: false,
    onSuccess: async value => { await cache.cancelQueries({ queryKey: receiptKey }); cache.setQueryData(receiptKey, value); },
  });
  const current = receipt.data ?? saved.value?.receipt;
  const fresh = !stale && !receipt.error && !!receipt.data;
  const blocked = stale || !computer.canTransferFiles || !!saved.value || !!saved.error || submit.isPending;
  function send(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (blocked) return;
    try {
      if (computer.accessMode === "granted" && !grant) throw new Error("Refresh your granted file access before transferring a file.");
      const location = retainedPath(path);
      const name = filename || location.split("/").at(-1)!;
      if (direction === "export" && (!name || name === "." || name === ".." || /[\\/\p{Cc}]/u.test(name)
        || new TextEncoder().encode(name).length > 255)) throw new Error("Use a filename without folders or control characters, up to 255 bytes.");
      const input: TransferFileInput = { computerId: id, requestId: uuidV7(), grantId: grant?.grantId ?? null,
        transfer: direction === "import" ? { kind: "import", artifactId: artifactId(source || reference), path: location }
          : { kind: "export", path: location, filename: name, mediaType: "application/octet-stream" },
        limits: { maximumBytes, maximumSeconds, onInterruption: "stop_computer" },
      };
      rememberFile(window.sessionStorage, key, input);
      setSaved({ value: { input } }); setError(undefined); submit.mutate(input);
    } catch (cause) { setError(cause instanceof Error ? cause.message : "The transfer could not be saved. It has not been sent."); }
  }
  function clear() {
    if (!fileFinished(current) && !window.confirm("Clearing this local request does not cancel the transfer. Review current work before starting another request.")) return;
    try { window.sessionStorage.removeItem(key); setSaved({}); setError(undefined); submit.reset(); }
    catch { setError("The saved transfer could not be cleared."); }
  }
  return <section className="computer-files" aria-label="Files">
    <div className="computers-toolbar"><h4>Files</h4>
      <button className="button button-secondary" onClick={onUpload}>Upload files</button></div>
    <p>Import an Artifact into the retained home, or export a file for download. Up to {formatBytes(maximumBytes)} per transfer.</p>
    {(error || saved.error || submit.error || receipt.error || cancel.error) && <p role="alert" className="computers-error">
      {error ?? saved.error ?? computerError(submit.error ?? receipt.error ?? cancel.error)}</p>}
    {current && <div className="computer-operation"><div>
      <p role="status">{stages[current.stage]}{current.stage === "completed" && !current.result ? "; confirming the receipt…" : ""}</p>
      {current.message && current.message !== stages[current.stage] && <p>{current.message}</p>}
      {current.cancellationRequestedAt && !fileFinished(current) && <p>Cancellation requested. Waiting for the confirmed outcome.</p>}
      {current.stage === "recovery_required" && <p>The original run is not confirmed stopped. This transfer keeps its execution slot; use Stop to end the Computer run and contact the installation operator for recovery.</p>}
      {!fresh && <p>Last confirmed transfer state. Refreshing requires current access.</p>}
      {current.result && fresh && <p>{formatBytes(current.result.bytes)} · <a className="button button-secondary" href={artifactDownloadUrl(current.result.artifactId)} download>Download Artifact</a></p>}
      <details><summary>Transfer reference</summary><code>{current.taskId}</code>
        {current.result && <><p>Artifact {current.result.artifactId}</p><p>SHA-256 {current.result.sha256}</p></>}</details>
      {current.canCancel && <button className="button button-secondary" disabled={!fresh || cancel.isPending}
        onClick={() => { if (window.confirm("Cancel this transfer? If it has started, this may stop all processes on the Computer. Your retained files stay.")) cancel.mutate(); }}>Cancel transfer</button>}
      <button className="button button-secondary" disabled={stale || receipt.isFetching} onClick={() => void receipt.refetch()}>Refresh transfer</button>
    </div></div>}
    {saved.value && !fileFinished(current) && (current?.stage === "queued" || !saved.value.receipt) && <div className="computer-operation">
      <p>{submit.isPending ? "Submitting the saved transfer…" : "Retry this saved request to recover its receipt or finish preparing Artifact access."}</p>
      <button className="button button-secondary" disabled={stale || submit.isPending} onClick={() => submit.mutate(saved.value!.input)}>Retry saved transfer</button>
    </div>}
    {(saved.value || saved.error) && <button className="button button-secondary" disabled={stale || submit.isPending || cancel.isPending} onClick={clear}>
      {fileFinished(current) ? "Dismiss completed transfer" : "Clear saved request after review"}</button>}
    <form onSubmit={send}><fieldset disabled={blocked}><legend>Transfer a file</legend>
      <label>Direction <select value={direction} onChange={event => setDirection(event.target.value as "import" | "export")}>
        <option value="import">Import to Computer</option><option value="export">Export to Artifacts</option></select></label>
      {direction === "import" && <>
        <label>Source Artifact <select value={source} onChange={event => {
          const selected = candidates.find(candidate => candidate.id === event.target.value);
          setSource(event.target.value); if (selected) setPath(selected.filename);
        }}><option value="">Enter an Artifact reference</option>
          {candidates.map(candidate => <option key={candidate.id} value={candidate.id}>{candidate.filename}{candidate.bytes === null ? "" : ` · ${formatBytes(candidate.bytes)}`} · {candidate.id.slice(-8)}</option>)}
        </select></label>
        {!sourceArtifact && <label>Artifact ID or URI <input value={reference} onChange={event => setReference(event.target.value)} placeholder="artifact://…" required /></label>}
      </>}
      <label>{direction === "import" ? "Destination in your home" : "File in your home"}
        <input value={path} onChange={event => setPath(event.target.value)} placeholder="project/data.csv" autoComplete="off" required /></label>
      {direction === "export" && <label>Download filename <input value={filename} onChange={event => setFilename(event.target.value)} placeholder={path.split("/").at(-1) || "data.bin"} autoComplete="off" /></label>}
      <p>Parent folders must already exist. Imports create a new file and never overwrite. Archives stay intact.</p>
      <button className="button button-primary" type="submit">{direction === "import" ? "Import file" : "Export file"}</button>
      <p className="subdued">Cancelling an active transfer may stop this Computer.</p>
    </fieldset></form>
    {!computer.canTransferFiles && !saved.value && <p>{computer.busy ? "Finish current work before starting another transfer."
      : computer.phase !== "ready" ? "Start the Computer before transferring files."
        : "File transfers need current access and a qualified environment. Review Environment below for an available update."}</p>}
  </section>;
}
