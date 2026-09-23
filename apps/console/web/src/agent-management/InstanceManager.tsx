import { useCallback, useEffect, useRef, useState } from "react";
import { uuidV7 } from "../agentControl";
import type { Authoring, Definition, InstanceChange, LifecycleOperation, ManagedInstance, PublishedRevision, UpdateInstance } from "../generated/agent-management";
import { AgentApi, AgentApiError } from "./api";

export function InstanceManager({ api, authoring, definitions, refreshVersion, openDefinition }: { api: AgentApi; authoring: Authoring; definitions: readonly Definition[]; refreshVersion: number; openDefinition: (id: string) => void }) {
  const [items, setItems] = useState<ManagedInstance[]>([]);
  const [next, setNext] = useState<string | null>();
  const [error, setError] = useState<string>();
  const [loaded, setLoaded] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const active = useRef(true);
  const refreshing = useRef(false);
  const again = useRef(false);
  const refresh = useCallback(async () => {
    if (refreshing.current) { again.current = true; return; }
    refreshing.current = true;
    try {
      do {
        again.current = false;
        try {
          const page = await api.instances();
          if (active.current) { setItems(page.items); setNext(page.next); setError(undefined); }
        } catch (e) { if (active.current) { setItems([]); setError(e instanceof Error ? e.message : String(e)); } }
        finally { if (active.current) setLoaded(true); }
      } while (active.current && again.current);
    } finally { refreshing.current = false; }
  }, [api]);
  useEffect(() => { active.current = true; void refresh(); return () => { active.current = false; }; }, [refresh, refreshVersion]);
  const visibleItems = items.filter(instance => showArchived || instance.desired !== "archived");
  return <section className="am-instances" aria-label="Managed instances"><header className="am-heading"><div><h2>Managed instances</h2><p>Run several instances from one published definition. Control each instance independently.</p></div><button onClick={() => void refresh()}>Refresh instances</button></header>
    <label className="am-check"><input type="checkbox" checked={showArchived} onChange={event => setShowArchived(event.target.checked)}/>Show archived instances</label>
    <p>Context capacity: {authoring.instanceLimit} retained instances and {authoring.storageLimitGib} GiB. Archived storage remains counted.</p>
    {error && <p className="am-error" role="alert">{error}</p>}
    {!loaded && <p role="status">Loading managed instances…</p>}
    {loaded && !visibleItems.length && !error && <p>{next ? "No matching instances loaded yet. Load more to continue." : "No managed instances match this view. To create one, choose a published managed definition in Definitions and select Deploy instance."}</p>}
    <div className="am-instance-grid">{visibleItems.map(instance => <InstanceCard key={instance.id} api={api} value={instance} authoring={authoring} definition={definitions.find(d => d.id === instance.definition)} openDefinition={() => openDefinition(instance.definition)} changed={() => void refresh()}/>)}</div>
    {next && <button onClick={() => void api.instances(next).then(page => { setItems(values => [...values, ...page.items.filter(v => !values.some(item => item.id === v.id))]); setNext(page.next); }).catch(e => setError(String(e.message ?? e)))}>Load more instances</button>}
  </section>;
}

function InstanceCard({ api, value, authoring, definition, openDefinition, changed }: { api: AgentApi; value: ManagedInstance; authoring: Authoring; definition?: Definition; openDefinition: () => void; changed: () => void }) {
  const publishedRevision = definition?.publishedDigest;
  const [operation, setOperation] = useState<LifecycleOperation>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [pending, setPending] = useState<UpdateInstance>();
  const [confirm, setConfirm] = useState<"stop" | "archive">();
  const [history, setHistory] = useState<PublishedRevision[]>();
  const [older, setOlder] = useState<string | null>();
  const [revision, setRevision] = useState("");
  const [reviewOpen, setReviewOpen] = useState(false);
  const canControl = authoring.permissions.instanceControl && value.desired !== "archived";
  const disabled = busy || !!pending;
  useEffect(() => {
    let current = true;
    void api.operation(value.operation).then(v => { if (current) setOperation(v); }).catch(e => { if (current) setError(String(e.message ?? e)); });
    return () => { current = false; };
  }, [api, value.operation, value.updatedAt]);
  useEffect(() => {
    if (!reviewOpen || !authoring.permissions.readContent) return;
    let current = true;
    void api.revisions(value.definition).then(page => {
      if (!current) return;
      setHistory(items => [...page.items, ...(items ?? []).filter(item => !page.items.some(v => v.digest === item.digest))]);
      setOlder(page.next);
      // Publication refreshes choices without changing the revision under review.
      setRevision(selected => selected || page.items[0]?.digest || "");
    }).catch(e => { if (current) setError(String(e.message ?? e)); });
    return () => { current = false; };
  }, [api, value.definition, publishedRevision, reviewOpen, authoring.permissions.readContent]);
  async function submit(change?: InstanceChange) {
    const request = pending ?? { requestId: uuidV7(), expectedGeneration: value.generation, change: change! };
    setPending(request); setBusy(true); setError(undefined);
    try {
      const result = await api.updateInstance(value.id, request);
      setOperation(result); setPending(undefined); setConfirm(undefined); changed();
    } catch (e) {
      if (e instanceof AgentApiError && e.status >= 400 && e.status < 500) { setPending(undefined); changed(); }
      setError(e instanceof Error ? e.message : String(e));
    } finally { setBusy(false); }
  }
  async function revisions(after?: string) {
    try {
      const page = await api.revisions(value.definition, after);
      setHistory(items => after ? [...items ?? [], ...page.items] : page.items); setOlder(page.next);
      if (!after) setRevision(page.items[0]?.digest ?? "");
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  const prior = history?.find(h => h.digest === value.requestedRevision);
  const proposed = history?.find(h => h.digest === revision);
  const fields = prior && proposed ? (["model", "instructions", "tools", "budgets", "execution"] as const).filter(key => JSON.stringify(prior.content[key]) !== JSON.stringify(proposed.content[key])) : undefined;
  return <article className="am-instance" aria-label={value.name}><header className="am-heading"><div><h3>{value.name}</h3><small>{value.id} · {value.workContext}</small></div><span role="status">{value.desired} requested · {value.observed}</span></header>
    <p>Definition: <button className="am-definition-link" onClick={openDefinition}>{definition?.name ?? value.definition}</button></p>
    <dl><dt>Revision</dt><dd>{value.activeRevision?.slice(7, 19) ?? "Not active"}{value.activeRevision !== value.requestedRevision && ` → ${value.requestedRevision.slice(7, 19)} requested`}</dd><dt>Generation</dt><dd>{value.activeGeneration} active / {value.generation} requested</dd><dt>Service identity</dt><dd>{value.clientId}</dd><dt>Storage retained</dt><dd>{value.storageGib} GiB</dd></dl>
    {operation && <p role="status">Operation {operation.phase}{operation.message ? `: ${operation.message}` : ""}</p>}
    {error && <p className="am-error" role="alert">{error}</p>}
    {pending && !busy && <p>The result is uncertain. <button onClick={() => void submit()}>Retry the same operation</button></p>}
    {canControl && <div className="am-actions">
      {value.desired === "running" ? <button disabled={disabled} onClick={() => void submit({ kind: "state", desired: "paused" })}>Pause</button> : authoring.permissions.deploy && <button disabled={disabled} onClick={() => void submit({ kind: "state", desired: "running" })}>Resume</button>}
      <button disabled={disabled} onClick={() => setConfirm("stop")}>Stop current run</button><button disabled={disabled} onClick={() => setConfirm("archive")}>Archive instance</button>
      {value.observed === "failed" && authoring.permissions.deploy && <button disabled={disabled} onClick={() => void submit({ kind: "retry" })}>Retry provisioning</button>}
    </div>}
    {confirm && <div className="am-review"><p>{confirm === "stop" ? "Stop further model and tool dispatch for the current run? Accepted Tasks and external operations retain their own cancellation controls." : "Archive this instance? Execution and credentials will close. Its memory, storage and audit history remain retained."}</p><button disabled={disabled} onClick={() => void submit(confirm === "stop" ? { kind: "stop" } : { kind: "state", desired: "archived" })}>Confirm {confirm}</button><button disabled={disabled} onClick={() => setConfirm(undefined)}>Cancel</button></div>}
    {canControl && authoring.permissions.deploy && authoring.permissions.readContent && <details onToggle={event => setReviewOpen(event.currentTarget.open)}><summary>Review a revision update</summary><p>The agent finishes its current run before switching. Identity, retained memory and accepted Tasks stay with this instance.</p>
      {history && <><label>Published revision<select disabled={disabled} value={revision} onChange={e => setRevision(e.target.value)}>{history.map(h => <option key={h.digest} value={h.digest}>{h.digest.slice(7, 19)} · {new Date(h.createdAt).toLocaleString()}{h.digest === value.requestedRevision ? " · Current request" : ""}</option>)}</select></label>
        {proposed && <div className="am-review"><p>Published by {proposed.createdBy}.</p><p>{fields ? `Changed: ${fields.join(", ") || "nothing"}.` : "Load the current revision to compare the complete changes before applying an update."}</p><p>Model: {proposed.content.model.id}. Tools: {proposed.content.tools.join(", ") || "None"}.</p><p>Limits: {proposed.content.budgets.maxCompletionCalls} model calls, {proposed.content.budgets.maxToolCalls} tool calls, {proposed.content.budgets.maxOutputTokens} output tokens, {proposed.content.budgets.deadlineSeconds} seconds.</p><details><summary>Proposed instructions</summary><pre>{proposed.content.instructions}</pre></details></div>}
        {older && <button onClick={() => void revisions(older)}>Load older revisions</button>}
        <button disabled={disabled || !prior || !proposed || revision === value.requestedRevision} onClick={() => void submit({ kind: "revision", revision })}>Apply reviewed revision</button>
      </>}
    </details>}
  </article>;
}
