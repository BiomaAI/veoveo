import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { uuidV7 } from "../agentControl";
import type { MaintenanceView, ResumeUpdateInput } from "../generated/computers";
import { computerError, resumeUpdate } from "./api";
import { readSavedResume, rememberResume, sendSavedResume } from "./recoveryRequest";

export function RecoveryPanel({ operation, scope, stale, onResumed, refresh }: {
  operation: MaintenanceView; scope: string; stale: boolean;
  onResumed: (receipt: MaintenanceView) => Promise<void>; refresh: () => void;
}) {
  const key = `veoveo.computers.recovery:${scope}:${operation.computerId}:${operation.taskId}`;
  const [saved, setSaved] = useState<{ input?: ResumeUpdateInput; error?: string }>(() => {
    try { return { input: readSavedResume(window.sessionStorage, key, operation.computerId, operation.taskId) }; }
    catch { return { error: "The saved recovery request cannot be read. Refresh and review the update before clearing it." }; }
  });
  const resume = useMutation({ mutationFn: (input: ResumeUpdateInput) => sendSavedResume(window.sessionStorage, key, input, resumeUpdate),
    retry: false, onSuccess: async ({ receipt, cleared }, input) => {
      setSaved(cleared ? {} : { input, error: "Recovery confirmed. The saved request could not be cleared locally; retrying it remains safe." });
      await onResumed(receipt);
    }, onError: () => refresh() });
  const blocked = stale || resume.isPending;
  function start() {
    if (blocked || saved.input || saved.error || !operation.canResume) return;
    const text = operation.pendingCancellationAt
      ? "Continue this environment update despite its pending cancellation? The original replacement and retained home stay the same. Another cancellation can still stop further steps."
      : "Resume this environment update with another limited recovery window? It will keep the original replacement and retained home.";
    if (!window.confirm(text)) return;
    const input: ResumeUpdateInput = { computerId: operation.computerId, taskId: operation.taskId,
      requestId: uuidV7(), expectedUpdatedAt: operation.updatedAt,
      acknowledgedCancellationAt: operation.pendingCancellationAt };
    try { rememberResume(window.sessionStorage, key, input); setSaved({ input }); resume.mutate(input); }
    catch { setSaved({ error: "The recovery request could not be saved. It has not been sent." }); }
  }
  if (operation.phase !== "recovery_required" && !saved.input && !saved.error) return null;
  return <div className="computer-recovery">
    {operation.phase === "recovery_required" && <p>Your home remains reserved. Resolve the reported cause before continuing this update.</p>}
    {saved.error && <p role="alert">{saved.error}</p>}
    {resume.error && <p role="alert">{computerError(resume.error)} Refresh the update to review its current state.</p>}
    {resume.isPending && <p role="status">Submitting the saved recovery request…</p>}
    {saved.input ? <>
      <p>The saved request refers to one recovery window. Retrying it cannot reopen a second window or override a newer cancellation.</p>
      <button className="button button-secondary" disabled={blocked} onClick={() => resume.mutate(saved.input!)}>Retry saved recovery</button>
    </> : operation.canResume && <button className="button button-secondary" disabled={blocked || !!saved.error} onClick={start}>
      {operation.pendingCancellationAt ? "Continue after cancellation request…" : "Resume update…"}
    </button>}
    {!saved.input && !operation.canResume && operation.phase === "recovery_required" && <p>Recovery is unavailable under the current policy or capacity. An authorized installation operator can check the cause.</p>}
    {(saved.input || saved.error) && <button className="button button-secondary" disabled={blocked} onClick={() => {
      if (!window.confirm("Review the current update first. Clearing this saved request does not cancel accepted recovery or release your home.")) return;
      try { window.sessionStorage.removeItem(key); setSaved({}); resume.reset(); refresh(); }
      catch { setSaved(previous => ({ ...previous, error: "The saved recovery request could not be cleared." })); }
    }}>Clear saved recovery after review</button>}
  </div>;
}
