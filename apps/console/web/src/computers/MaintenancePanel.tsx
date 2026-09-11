import { useEffect, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { uuidV7 } from "../agentControl";
import type { ComputerSnapshot, MaintenancePhase, MaintenanceRecoveryReason } from "../generated/computers";
import { computerError, readMaintenance, readMaintenanceOperation, updateTemplate } from "./api";
import { readSavedUpdate, rememberUpdate, sendSavedUpdate, updateFinished, type SavedUpdate } from "./maintenanceRequest";

const phases: Record<MaintenancePhase, string> = {
  queued: "Update accepted",
  stopping: "Stopping processes",
  saving_policy: "Saving access policy",
  replacing: "Replacing the environment; files are retained",
  starting: "Starting the updated environment",
  restoring_policy: "Restoring access policy",
  verifying: "Verifying the updated environment",
  succeeded: "Environment updated. Your retained files are available.",
  cancelled: "Update cancelled before the environment changed.",
  recovery_required: "Update paused for recovery",
};
const recovery: Record<MaintenanceRecoveryReason, string> = {
  observation_budget_exhausted: "The outcome could not be confirmed within the recovery window.",
  authority_denied: "Current policy prevents continuing this update.",
  cancellation_requested: "Cancellation was requested after the update began.",
  checkpoint_unavailable: "The saved access policy could not be opened or verified.",
};
export function MaintenancePanel({ computerId, scope, snapshot, stale }: {
  computerId: string; scope: string; snapshot: ComputerSnapshot; stale: boolean;
}) {
  const cache = useQueryClient();
  const key = `veoveo.computers.update:${scope}:${computerId}`;
  const [saved, setSaved] = useState<{ value?: SavedUpdate; error?: string }>(() => {
    try { return { value: readSavedUpdate(window.sessionStorage, key, computerId) }; }
    catch { return { error: "The saved update cannot be read. Review the current environment before clearing it." }; }
  });
  const [selected, select] = useState("");
  const [error, setError] = useState<string>();
  const inventory = useQuery({ queryKey: ["computers", "maintenance", computerId],
    queryFn: ({ signal }) => readMaintenance(computerId, signal), enabled: !stale,
    retry: false, refetchOnWindowFocus: false });
  const task = saved.value?.receipt?.taskId;
  const receipt = useQuery({ queryKey: ["computers", "maintenance-receipt", computerId, task],
    queryFn: ({ signal }) => readMaintenanceOperation(computerId, task!, signal),
    enabled: !!task && !stale, retry: false, refetchOnWindowFocus: false });
  useEffect(() => {
    void cache.invalidateQueries({ queryKey: ["computers", "maintenance", computerId] });
    void cache.invalidateQueries({ queryKey: ["computers", "maintenance-receipt", computerId] });
  }, [cache, computerId, snapshot]);
  const update = useMutation({ mutationFn: (input: SavedUpdate["input"]) =>
    sendSavedUpdate(window.sessionStorage, key, input, updateTemplate), retry: false,
    onSuccess: async (result, input) => {
      const receiptKey = ["computers", "maintenance-receipt", computerId, result.receipt.taskId];
      await cache.cancelQueries({ queryKey: receiptKey });
      cache.setQueryData(receiptKey, result.receipt);
      setSaved({ value: { input, receipt: result.receipt }, error: result.saved ? undefined
        : "Update confirmed. Its receipt could not be saved locally; retain the original request for recovery." });
      void cache.invalidateQueries({ queryKey: ["computers", "maintenance", computerId] });
    } });
  const current = receipt.data ?? saved.value?.receipt;
  const active = inventory.data?.active;
  const targets = inventory.data?.targets ?? [];
  const target = targets.find(t => t.templateId === selected)
    ?? targets.find(t => t.templateId === snapshot.template?.templateId) ?? targets[0];
  const blocked = stale || update.isPending || !!saved.value || !!saved.error || !inventory.data?.canUpdate;
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (blocked || !target) return;
    if (!window.confirm(`Update to ${target.templateId}? Running processes will stop. Your retained home and files stay.`)) return;
    const input = { computerId, requestId: uuidV7(), templateId: target.templateId };
    try {
      rememberUpdate(window.sessionStorage, key, input);
      setSaved({ value: { input } }); setError(undefined); update.mutate(input);
    } catch { setError("The update request could not be saved. It has not been sent."); }
  }
  const visible = active ?? current;
  return <section className="computer-maintenance" aria-label="Environment updates">
    <div className="computers-toolbar"><h4>Environment</h4>
      <button className="button button-secondary" disabled={stale || inventory.isFetching || receipt.isFetching}
        onClick={() => { void inventory.refetch(); if (task) void receipt.refetch(); }}>Refresh update</button></div>
    <p>Update the environment while keeping your home, repositories and files. Running processes stop during an update.</p>
    {(inventory.error || receipt.error || update.error) && <p role="alert" className="computers-error">{computerError(update.error ?? receipt.error ?? inventory.error)}</p>}
    {(error || saved.error) && <p role="alert">{error ?? saved.error}</p>}
    {visible && <div className="computer-operation"><div>
      <p role="status">{phases[visible.phase]}{stale ? " Last confirmed state; live updates are reconnecting." : ""}</p>
      <p>{visible.sourceTemplateId} → {visible.targetTemplateId}</p>
      {visible.recovery && <p role="alert">{recovery[visible.recovery]} Your home remains reserved. An installation operator must resolve this operation before another update can begin.</p>}
      <details><summary>Update reference</summary><code>{visible.taskId}</code></details>
    </div></div>}
    {saved.value && !updateFinished(current) && <div className="computer-operation"><div>
      <p>{update.isPending ? "Submitting the saved update…" : saved.value.receipt
        ? "This request keeps its original environment selection." : "The update outcome is unconfirmed. Retry this saved request to recover its status."}</p>
      <small>Request {saved.value.input.requestId}</small></div>
      {!saved.value.receipt && <button className="button button-secondary" disabled={stale || update.isPending}
        onClick={() => update.mutate(saved.value!.input)}>Retry saved update</button>}
    </div>}
    {(saved.value || saved.error) && <button className="button button-secondary" disabled={update.isPending || stale}
      onClick={() => {
        if (!updateFinished(current) && !window.confirm("Review the current update first. Clearing this saved request does not cancel an accepted update or release its retained home.")) return;
        try { window.sessionStorage.removeItem(key); setSaved({}); setError(undefined); update.reset(); }
        catch { setError("The saved update could not be cleared."); }
      }}>{updateFinished(current) ? "Dismiss completed update" : "Clear saved request after review"}</button>}
    {inventory.isPending && <p>Loading environment updates…</p>}
    {!inventory.isPending && !inventory.error && !active && targets.length === 0 && <p>No environment update is currently available for this Computer.</p>}
    {targets.length > 0 && !active && <form onSubmit={submit}>
      <fieldset disabled={blocked}><legend>Update environment</legend>
        <label>Environment <select value={target?.templateId ?? ""} onChange={event => select(event.target.value)}>
          {targets.map(template => <option key={template.templateId} value={template.templateId}>{template.templateId}</option>)}
        </select></label>
        <button className="button button-secondary" type="submit">Update environment</button>
      </fieldset>
      {!inventory.data?.canUpdate && <p>Finish active commands and wait for current operations and capacity before updating.</p>}
    </form>}
  </section>;
}
