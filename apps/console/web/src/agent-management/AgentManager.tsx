import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { uuidV7 } from "../agentControl";
import type { Authoring, CreateDefinition, Definition } from "../generated/agent-management";
import { AgentApi, AgentApiError } from "./api";
import { initialContent } from "./ContentEditor";
import { DefinitionEditor } from "./DefinitionEditor";
import "./style.css";

export function AgentManager({ app }: { app: "console" | "workspace" }) {
  const api = useMemo(() => new AgentApi(app), [app]);
  const [authoring, setAuthoring] = useState<Authoring>();
  const [definitions, setDefinitions] = useState<Definition[]>([]);
  const [next, setNext] = useState<string | null>();
  const [selected, setSelected] = useState<string>();
  const [creating, setCreating] = useState(false);
  const [source, setSource] = useState<Definition>();
  const [error, setError] = useState<string>();
  const [loaded, setLoaded] = useState(false);
  const active = useRef(true);
  const loading = useRef(false);
  const refreshAgain = useRef(false);
  const selectedRef = useRef(selected); selectedRef.current = selected;
  const refresh = useCallback(async () => {
    if (loading.current) { refreshAgain.current = true; return; }
    loading.current = true;
    try {
      const [auth, page] = await Promise.all([api.authoring(), api.list()]);
      if (!active.current) return;
      const picked = selectedRef.current;
      const selectedValue = picked && !page.items.some(d => d.id === picked) ? await api.read(picked).catch(() => undefined) : undefined;
      setAuthoring(auth); setDefinitions(selectedValue ? [...page.items, selectedValue] : page.items); setNext(page.next); setError(undefined);
      if (picked && !selectedValue && !page.items.some(d => d.id === picked)) setSelected(undefined);
    } catch (e) {
      if (!active.current) return;
      if (e instanceof AgentApiError && [401, 403, 404].includes(e.status)) { setAuthoring(undefined); setDefinitions([]); setSelected(undefined); }
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      loading.current = false;
      if (active.current) setLoaded(true);
      if (active.current && refreshAgain.current) { refreshAgain.current = false; void refresh(); }
    }
  }, [api]);
  useEffect(() => { active.current = true; void refresh(); const close = api.observe(() => void refresh()); return () => { active.current = false; close(); }; }, [api, refresh]);
  const current = definitions.find(d => d.id === selected);
  return <section className="agent-management" aria-label="Agent definitions"><header className="am-heading"><div><h2>Agent definitions</h2><p>Create, revise and publish agents for your Work Context.</p></div><div className="am-actions"><button onClick={() => void refresh()}>Refresh</button>{authoring?.permissions.create && <button className="am-primary" disabled={!authoring.models.length} onClick={() => { setSource(undefined); setCreating(true); }}>Create agent</button>}</div></header>
    {error && <p className="am-error" role="alert">{error}</p>}
    {!loaded && <p role="status">Loading agent definitions…</p>}
    {authoring && !authoring.models.length && <p>An operator must admit a model connection before agents can be published.</p>}
    {authoring && <div className="am-layout"><aside className="am-list" aria-label="Choose an agent">{definitions.length ? definitions.map(d => <button key={d.id} aria-pressed={d.id === selected} onClick={() => setSelected(d.id)}><strong>{d.name}</strong><small>{d.description}</small><span>{d.status} · {d.publishedDigest ? "Published" : "Draft"}</span></button>) : <p>No definitions are available for you to manage here.</p>}
      {next && <button onClick={() => void api.list(next).then(page => { setDefinitions(values => [...values, ...page.items.filter(d => !values.some(v => v.id === d.id))]); setNext(page.next); }).catch(e => setError(e.message))}>Load more</button>}
    </aside>{current ? <DefinitionEditor key={`${current.id}:${authoring.permissions.readContent}`} api={api} initial={current} authoring={authoring} changed={() => void refresh()} duplicate={value => { setSource(value); setCreating(true); }}/>
      : <div className="am-empty"><h3>Give an agent a clear purpose.</h3><p>Choose a model, write instructions and select the capabilities it can use. Publishing makes it available to chat owners in its admitted contexts.</p></div>}</div>}
    {creating && authoring && <CreateAgent api={api} authoring={authoring} source={source} close={() => setCreating(false)} created={definition => { setDefinitions(values => [definition, ...values.filter(v => v.id !== definition.id)]); setSelected(definition.id); setCreating(false); void refresh(); }}/>}
  </section>;
}

function CreateAgent({ api, authoring, source, close, created }: { api: AgentApi; authoring: Authoring; source?: Definition; close: () => void; created: (definition: Definition) => void }) {
  const [name, setName] = useState(source ? `${source.name} copy` : "");
  const [id, setId] = useState("");
  const [customId, setCustomId] = useState(false);
  const [description, setDescription] = useState(source?.description ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const pending = useRef<CreateDefinition | undefined>(undefined);
  async function create() {
    setBusy(true); setError(undefined);
    try {
      pending.current ??= { requestId: uuidV7(), id, name, description,
        source: source?.publishedDigest ? { kind: "duplicate", definition: source.id, digest: source.publishedDigest }
          : { kind: "blank", content: initialContent(authoring) } };
      created(await api.create(pending.current));
    } catch (e) {
      if (e instanceof AgentApiError && e.status >= 400 && e.status < 500) pending.current = undefined;
      setError(e instanceof Error ? e.message : String(e));
    } finally { setBusy(false); }
  }
  return <dialog className="am-modal" ref={node => { if (node && !node.open) node.showModal(); }} onCancel={event => { if (busy || pending.current) event.preventDefault(); else close(); }} aria-labelledby="am-create-title"><form onSubmit={event => { event.preventDefault(); void create(); }}><h2 id="am-create-title">{source ? "Duplicate agent" : "Create chat assistant"}</h2><p>{source ? "Copies the published configuration. Credentials, memory, participants and grants stay with their owners." : "Start with a private draft. You can review the configuration before publishing."}</p>
    <fieldset disabled={busy || !!pending.current}><label>Name<input autoFocus required maxLength={200} value={name} onChange={e => { setName(e.target.value); if (!customId) setId(e.target.value.toLowerCase().replace(/[^a-z0-9_-]+/g, "-")); }}/></label><label>Agent ID<input required pattern="[a-z0-9_-]+" maxLength={128} value={id} onChange={e => { setCustomId(true); setId(e.target.value); }} placeholder="research-assistant"/></label><label>Description<textarea maxLength={2000} required rows={3} value={description} onChange={e => setDescription(e.target.value)}/></label></fieldset>
    {error && <p className="am-error" role="alert">{error}</p>}{pending.current && !busy && <p>The result is uncertain. Retry this request to recover its outcome before changing the form.</p>}
    <div className="am-actions"><button disabled={busy || !!pending.current} type="button" onClick={close}>Cancel</button><button className="am-primary" disabled={busy || !name.trim() || !id || !description.trim()}>{busy ? "Creating…" : pending.current ? "Retry creation" : "Create draft"}</button></div>
  </form></dialog>;
}
