import { useEffect, useRef, useState } from "react";
import { uuidV7 } from "../agentControl";
import type { Authoring, CapabilityChoice, Content, Definition, PublishedRevision, Validation, TemplateChoice } from "../generated/agent-management";
import { AgentApi } from "./api";
import { DeployInstance } from "./DeployInstance";
import { ContentEditor } from "./ContentEditor";

export function DefinitionEditor({ api, initial, authoring, templates, changed, deployed, duplicate }: {
  api: AgentApi; initial: Definition; authoring: Authoring; templates: TemplateChoice[]; changed: () => void; deployed: () => void; duplicate: (definition: Definition) => void;
}) {
  const [deploying, setDeploying] = useState(false);
  const [definition, setDefinition] = useState(initial);
  const [content, setContent] = useState<Content>();
  const [saved, setSaved] = useState<Content>();
  const [history, setHistory] = useState<PublishedRevision[]>([]);
  const [next, setNext] = useState<string | null>();
  const [capabilities, setCapabilities] = useState<CapabilityChoice[]>();
  const [name, setName] = useState(initial.name);
  const [description, setDescription] = useState(initial.description);
  const [audience, setAudience] = useState((initial.audience.length ? initial.audience : [authoring.workContext]).join(", "));
  const [owner, setOwner] = useState("");
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [review, setReview] = useState<Validation>();
  const [confirm, setConfirm] = useState<"disable" | "archive">();
  const requests = useRef(new Map<string, string>());
  const current = useRef(true);
  const permission = authoring.permissions;
  const dirty = content !== undefined && JSON.stringify(content) !== JSON.stringify(saved);
  const id = definition.id;
  const requestId = (action: string, value: unknown) => {
    const key = JSON.stringify([id, action, definition.revision, value]);
    let request = requests.current.get(key);
    if (!request) { request = uuidV7(); requests.current.set(key, request); }
    return request;
  };
  async function load() {
    const metadata = await api.read(id);
    const [draft, revisions] = permission.readContent ? await Promise.all([api.draft(id), api.revisions(id)]) : [];
    if (!current.current) return;
    setDefinition(metadata); setName(metadata.name); setDescription(metadata.description);
    setAudience((metadata.audience.length ? metadata.audience : [authoring.workContext]).join(", "));
    setContent(draft?.content); setSaved(draft?.content); setHistory(revisions?.items ?? []); setNext(revisions?.next);
    setReview(undefined); setError(undefined);
  }
  useEffect(() => {
    current.current = true;
    void load().catch(e => { if (current.current) setError(String(e.message ?? e)); });
    return () => { current.current = false; };
    // This component is keyed by definition ID. Reload is explicit when drafts conflict.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  async function act(label: string, action: () => Promise<void>) {
    setBusy(label); setError(undefined); setNotice(undefined);
    try { await action(); changed(); }
    catch (e) { if (current.current) setError(e instanceof Error ? e.message : String(e)); }
    finally { if (current.current) setBusy(undefined); }
  }
  async function update(action: "enable" | "disable" | "archive") {
    const value = { expectedRevision: definition.revision };
    const result = await api.status(id, action, { ...value, requestId: requestId(action, value) });
    if (current.current) { setDefinition(result); setConfirm(undefined); setNotice(`Agent ${action === "enable" ? "enabled" : action === "disable" ? "disabled" : "archived"}.`); }
  }
  const published = history.find(h => h.digest === definition.publishedDigest);
  const differences = content ? (Object.keys(content) as (keyof Content)[]).filter(key => JSON.stringify(content[key]) !== JSON.stringify(published?.content[key])) : [];
  return <article className="am-editor">
    <header className="am-heading"><div><h2>{definition.name}</h2><p>{definition.status} · Revision {definition.revision}{definition.publishedDigest ? " · Published" : " · Draft only"}</p></div>
      <button disabled={!!busy} onClick={() => void act("Reloading", load)}>Reload</button></header>
    {initial.revision > definition.revision && <p className="am-warning">A newer edit is available. Reload before saving; your current text stays here until you do.</p>}
    {error && <p role="alert" className="am-error">{error}</p>}{notice && <p role="status">{notice}</p>}
    <fieldset disabled={!!busy || !permission.edit || definition.status === "archived"} className="am-content">
      <label>Name<input value={name} maxLength={200} onChange={e => setName(e.target.value)}/></label>
      <label>Description<textarea value={description} maxLength={2000} rows={2} onChange={e => setDescription(e.target.value)}/></label>
      {permission.edit && <button disabled={name === definition.name && description === definition.description} onClick={() => void act("Saving details", async () => {
        const change = { kind: "presentation" as const, name, description };
        const result = await api.metadata(id, { requestId: requestId("metadata", change), expectedRevision: definition.revision, change });
        setDefinition(result); setReview(undefined); setNotice("Details saved.");
      })}>Save details</button>}
    </fieldset>
    {content ? <><ContentEditor authoring={authoring} templates={templates} capabilities={capabilities} value={content} change={v => { setContent(v); setReview(undefined); }} disabled={!!busy || !permission.edit || definition.status === "archived"}/>
      <div className="am-actions"><button disabled={!!busy} onClick={() => void act("Loading capabilities", async () => setCapabilities(await api.capabilities()))}>Load capabilities</button>
        {permission.edit && <button className="am-primary" disabled={!!busy || !dirty || definition.status === "archived"} onClick={() => void act("Saving draft", async () => {
          const result = await api.save(id, { requestId: requestId("draft", content), expectedRevision: definition.revision, content });
          setDefinition(result); setSaved(content); setReview(undefined); setNotice("Draft saved. Published agents keep their current instructions.");
        })}>Save draft</button>}
      </div></> : <p>{permission.readContent ? "Loading draft…" : "Your permissions allow metadata access. Private instructions require content access."}</p>}
    {permission.publish && definition.status !== "archived" && <section className="am-publication"><h3>Publish</h3>
      <p>Existing participants and managed instances keep their revision until an authorized owner applies an update.</p>
      <label>Work Contexts<input value={audience} disabled={!!busy} onChange={e => { setAudience(e.target.value); setReview(undefined); }} aria-describedby="am-audience-help"/></label>
      <small id="am-audience-help">Separate context IDs with commas. Publication into another context requires authority there.</small>
      <button disabled={!!busy || dirty || definition.disabled} onClick={() => void act("Validating publication", async () => {
        const result = await api.validate(id, { expectedRevision: definition.revision, audience: audience.split(",").map(v => v.trim()).filter(Boolean) });
        setReview(result);
      })}>Review publication</button>
      {dirty && <p>Save your draft before reviewing publication.</p>}
      {review && <div className="am-review"><p>Revision {review.revision} · {review.digest.slice(0, 19)}…</p>
        <p>{published ? `Changed: ${differences.join(", ") || "publication audience only"}.` : "This publishes the saved instructions, model, capabilities and budgets."}</p>
        {review.findings.length ? <ul>{review.findings.map((f, i) => <li key={i}>{f.message}</li>)}</ul> : <><p>Validation passed. This version will become available in the selected contexts.</p><button className="am-primary" disabled={!!busy || dirty} onClick={() => void act("Publishing", async () => {
          const value = { expectedRevision: definition.revision, digest: review.digest, audience: audience.split(",").map(v => v.trim()).filter(Boolean) };
          const result = await api.publish(id, { ...value, requestId: requestId("publish", value) });
          setDefinition(result); setReview(undefined); setNotice("Published. Chats that already include this agent keep the revision they use now.");
          const revisions = await api.revisions(id); setHistory(revisions.items); setNext(revisions.next);
        })}>Publish this revision</button></>}
      </div>}
    </section>}
    <section><h3>Published history</h3>{history.length ? <ol className="am-history">{history.map(h => <li key={h.digest}><code>{h.digest.slice(7, 19)}</code> · {new Date(h.createdAt).toLocaleString()}{h.digest === definition.publishedDigest ? " · Current" : ""}<details><summary>Inspect revision</summary><pre>{h.content.instructions}</pre><p>Model: {h.content.model.id}</p><p>Capabilities: {h.content.tools.join(", ") || "None"}</p></details></li>)}</ol> : <p>No published revisions available.</p>}
      {next && <button disabled={!!busy} onClick={() => void act("Loading history", async () => { const page = await api.revisions(id, next); setHistory(values => [...values, ...page.items]); setNext(page.next); })}>Load older revisions</button>}
    </section>
    <div className="am-actions">
      {permission.deploy && published?.content.execution.kind === "managed" && definition.status === "enabled" && !definition.disabled && <button disabled={!!busy} onClick={() => setDeploying(true)}>Deploy instance</button>}
      {permission.create && permission.readContent && definition.publishedDigest && <button disabled={!!busy} onClick={() => duplicate(definition)}>Duplicate published version</button>}
      {permission.control && definition.status !== "archived" && <button disabled={!!busy} onClick={() => definition.disabled ? void act("Enabling", () => update("enable")) : setConfirm("disable")}>{definition.disabled ? "Enable" : "Disable"}</button>}
      {permission.archive && definition.status !== "archived" && <button disabled={!!busy} onClick={() => setConfirm("archive")}>Archive</button>}
    </div>
    {confirm && <div className="am-review" role="group" aria-label="Confirm agent status"><p>{confirm === "disable" ? "Disable this agent? Running copies stop, including ones on older revisions." : "Archive this definition? It can no longer be added to chats. Chats that already include it can keep using their current revision."}</p><button disabled={!!busy} onClick={() => void act("Updating status", () => update(confirm))}>Confirm {confirm}</button><button disabled={!!busy} onClick={() => setConfirm(undefined)}>Cancel</button></div>}
    {permission.transfer && <details><summary>Transfer ownership</summary><p>The new owner must be an active user or service account in this tenant. Transferring ownership doesn't give them access to the Work Context.</p><label>User or service account ID<input value={owner} onChange={e => setOwner(e.target.value)}/></label><button disabled={!!busy || !/^[0-9a-f-]{36}$/i.test(owner)} onClick={() => void act("Transferring ownership", async () => {
      await api.metadata(id, { requestId: requestId("transfer", owner), expectedRevision: definition.revision, change: { kind: "transfer", owner } });
      changed(); setNotice("Ownership transferred.");
    })}>Transfer</button></details>}
    {deploying && published && <DeployInstance api={api} definition={definition} revision={published} templates={templates} close={() => setDeploying(false)} created={operation => { setDeploying(false); setNotice(`Instance ${operation.instance} accepted. Follow provisioning in Instances.`); changed(); deployed(); }}/>}
    {busy && <p role="status">{busy}…</p>}
  </article>;
}
