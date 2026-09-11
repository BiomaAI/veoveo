import { useEffect, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { uuidV7 } from "../agentControl";
import { parseComputer } from "../generatedContracts";
import type { ComputerSnapshot, IssueAutomationGrantInput } from "../generated/computers";
import { useSnapshot } from "../queries";
import { identityLabel } from "../identity";
import { computerError, grantAutomation, readAutomation, revokeAutomation } from "./api";

export function AutomationPanel({ computerId, scope, snapshot, stale }: {
  computerId: string; scope: string; snapshot: ComputerSnapshot; stale: boolean;
}) {
  const cache = useQueryClient();
  const directory = useSnapshot(false);
  const key = `veoveo.computers.automation:${scope}:${computerId}`;
  const [saved, setSaved] = useState<{ input?: IssueAutomationGrantInput; error?: string }>(() => {
    try {
      const text = window.sessionStorage.getItem(key);
      if (!text) return {};
      const input = parseComputer("issue_automation_grant", JSON.parse(text));
      if (input.computerId !== computerId) throw new Error("wrong Computer");
      return { input };
    } catch { return { error: "The saved grant request cannot be read. Review current grants before clearing it." }; }
  });
  const [principal, setPrincipal] = useState("");
  const [clientId, setClientId] = useState("");
  const [name, setName] = useState("");
  const [consent, setConsent] = useState(false);
  const [formError, setFormError] = useState<string>();
  const queryKey = ["computers", "automation", computerId];
  const inventory = useQuery({ queryKey, queryFn: ({ signal }) => readAutomation(computerId, signal),
    enabled: !stale, retry: false, refetchOnWindowFocus: false });
  useEffect(() => {
    void cache.invalidateQueries({ queryKey: ["computers", "automation", computerId] });
  }, [cache, computerId, snapshot]);
  const issue = useMutation({ mutationFn: grantAutomation, retry: false,
    onSuccess: () => {
      try { window.sessionStorage.removeItem(key); setSaved({}); }
      catch { setSaved(previous => ({ ...previous, error: "Grant confirmed. Its saved request could not be cleared." })); }
      setConsent(false);
      void cache.invalidateQueries({ queryKey: ["computers", "automation", computerId] });
    } });
  const revoke = useMutation({ mutationFn: (grant: string) => revokeAutomation(computerId, grant), retry: false,
    onSuccess: () => cache.invalidateQueries({ queryKey: ["computers", "automation", computerId] }) });
  const limits = inventory.data?.limits;
  const blocked = stale || issue.isPending || !!saved.input || !!saved.error || !inventory.data?.canGrant;
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (blocked || !limits || !consent) return;
    const fields = new FormData(event.currentTarget);
    const lifetime = Number(fields.get("lifetime"));
    try {
      if (!Number.isInteger(lifetime) || lifetime < 1 || lifetime > limits.maximumLifetimeSeconds) throw new Error("invalid duration");
      const input = parseComputer("issue_automation_grant", {
        computerId, requestId: uuidV7(), principalId: principal.trim(), oauthClientId: clientId.trim(), name: name.trim(),
        permissions: ["execute"], expiresAt: new Date(Date.now() + lifetime * 1000).toISOString(),
        executionLimits: { maximumSeconds: Number(fields.get("seconds")), maximumOutputBytes: Number(fields.get("bytes")), onInterruption: "stop_computer" },
      });
      window.sessionStorage.setItem(key, JSON.stringify(input));
      setSaved({ input }); setFormError(undefined); issue.mutate(input);
    } catch { setFormError("Check the grant values and browser storage. The request has not been sent."); }
  }
  const durations = limits ? [...new Set([60, 900, 3600, 28800, limits.maximumLifetimeSeconds])]
    .filter(seconds => seconds <= limits.maximumLifetimeSeconds).sort((a, b) => a - b) : [];
  return <section className="computer-automation" aria-label="Agent access">
    <div className="computers-toolbar"><h4>Agent access</h4>
      <button className="button button-secondary" disabled={inventory.isFetching || stale} onClick={() => void inventory.refetch()}>Refresh grants</button></div>
    <p>Let a named agent run bounded commands in this Computer. Each command returns a Task and governed output Artifacts.</p>
    {(inventory.error || issue.error || revoke.error) && <p className="computers-error" role="alert">{computerError(issue.error ?? revoke.error ?? inventory.error)}</p>}
    {(formError || saved.error) && <p role="alert">{formError ?? saved.error}</p>}
    {issue.data && <p role="status">Grant confirmed for {issue.data.grant.name}. Grant ID: <code>{issue.data.grant.grantId}</code></p>}
    {revoke.isSuccess && <p role="status">Grant revoked. Any active command is being contained under its interruption scope.</p>}
    {saved.input && <div className="computer-operation"><div><strong>Saved grant request: {saved.input.name}</strong>
      <p>Keep this request until its outcome is confirmed. Retrying preserves its original scope and expiry.</p></div>
      <button className="button button-secondary" disabled={issue.isPending || stale} onClick={() => issue.mutate(saved.input!)}>Retry saved request</button></div>}
    {(saved.input || saved.error) && <button className="button button-secondary" disabled={issue.isPending} onClick={() => {
      if (!window.confirm("Review the current grant inventory first. Clearing this request does not revoke a grant that may already have been issued.")) return;
      try { window.sessionStorage.removeItem(key); setSaved({}); issue.reset(); }
      catch { setFormError("The saved request could not be cleared."); }
    }}>Clear request after review</button>}
    {inventory.isPending && <p>Loading agent access…</p>}
    {inventory.data?.grants.length === 0 && <p>No active automation grants.</p>}
    {inventory.data?.grants.map(grant => <div className="computer-operation" key={grant.grantId}>
      <div><strong>{grant.name}</strong><p>{directory.data ? identityLabel(grant.principalId, directory.data) : grant.principalId} · {grant.permissions.join(", ")}</p>
        <small>Application {grant.oauthClientId} · Expires {new Date(grant.expiresAt).toLocaleString()}</small>
        {grant.executionLimits && <p>Each command: up to {grant.executionLimits.maximumSeconds} seconds and {grant.executionLimits.maximumOutputBytes.toLocaleString()} output bytes.</p>}
        <details><summary>Grant reference</summary><code>{grant.grantId}</code></details>
      </div>
      <button className="button button-secondary" disabled={stale || revoke.isPending || !inventory.data.canRevoke} onClick={() => {
        if (window.confirm("Revoke this agent grant? Interrupting an active command can stop all processes on this Computer. Retained files stay.")) revoke.mutate(grant.grantId);
      }}>Revoke agent access</button>
    </div>)}
    {limits && <details><summary>Grant command access</summary><form className="computer-automation-form" onSubmit={submit}>
      <fieldset disabled={blocked}><legend>Agent and scope</legend>
        <label>Grant name<input value={name} onChange={e => setName(e.target.value)} maxLength={64} required /></label>
        <label>Principal<input list="computer-agent-principals" value={principal} onChange={e => setPrincipal(e.target.value)} maxLength={2048} required /></label>
        <datalist id="computer-agent-principals">{directory.data?.principals.map(p => <option key={p.id} value={p.id}>{p.displayName}</option>)}</datalist>
        <label>Application client ID<input value={clientId} onChange={e => setClientId(e.target.value)} maxLength={256} required /></label>
        <p>Use the client ID that this principal signs in through. The grant applies only to that principal and application.</p>
        <label>Access duration<select name="lifetime" defaultValue={Math.min(3600, limits.maximumLifetimeSeconds)}>{durations.map(seconds => <option key={seconds} value={seconds}>{seconds >= 3600 ? `${seconds / 3600} hours` : seconds >= 60 ? `${seconds / 60} minutes` : `${seconds} seconds`}</option>)}</select></label>
        <label>Maximum seconds per command<input name="seconds" type="number" min={1} max={limits.maximumExecutionSeconds} defaultValue={Math.min(60, limits.maximumExecutionSeconds)} required /></label>
        <label>Maximum output bytes per command<input name="bytes" type="number" min={1} max={limits.maximumOutputBytes} defaultValue={Math.min(1048576, limits.maximumOutputBytes)} required /></label>
        <label className="computer-pairing-check"><input type="checkbox" checked={consent} onChange={e => setConsent(e.target.checked)} required />I allow an interrupted command to stop all processes on this Computer. Retained files stay.</label>
        <button className="button button-primary" disabled={!consent}>Grant command access</button>
      </fieldset>
      {!inventory.data?.canGrant && <p>Current permissions or the grant limit prevent issuing another grant.</p>}
    </form></details>}
  </section>;
}
