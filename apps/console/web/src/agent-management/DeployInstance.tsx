import { useState } from "react";
import { uuidV7 } from "../agentControl";
import type { Definition, PublishedRevision, TemplateChoice, LifecycleOperation, ProvisionInstance } from "../generated/agent-management";
import { AgentApi, AgentApiError } from "./api";
import { TemplateAuthority } from "./TemplateFields";

export function DeployInstance({ api, definition, revision, templates, close, created }: {
  api: AgentApi; definition: Definition; revision: PublishedRevision; templates: TemplateChoice[];
  close: () => void; created: (operation: LifecycleOperation) => void;
}) {
  const [name, setName] = useState(definition.name);
  const [id, setId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [pending, setPending] = useState<ProvisionInstance>();
  const execution = revision.content.execution;
  const template = execution.kind === "managed" ? templates.find(t => t.id === execution.template && t.revision === execution.templateRevision) : undefined;
  async function deploy() {
    setBusy(true); setError(undefined);
    try {
      const request = pending ?? { requestId: uuidV7(), id, name, definition: definition.id, revision: revision.digest };
      setPending(request);
      created(await api.provision(request));
    } catch (e) {
      if (e instanceof AgentApiError && e.status >= 400 && e.status < 500) setPending(undefined);
      setError(e instanceof Error ? e.message : String(e));
    } finally { setBusy(false); }
  }
  return <dialog className="am-modal" ref={node => { if (node && !node.open) node.showModal(); }} aria-labelledby="am-deploy-title" onCancel={e => { if (busy || pending) e.preventDefault(); else close(); }}><form onSubmit={e => { e.preventDefault(); void deploy(); }}>
    <h2 id="am-deploy-title">Deploy {definition.name}</h2><p>This creates a persistent agent using published revision {revision.digest.slice(7, 19)}. Its memory and service identity belong to this instance.</p>
    {template ? <TemplateAuthority template={template}/> : <p role="alert">The published runtime template is unavailable. Publish an admitted configuration before deploying.</p>}
    {execution.kind === "managed" && <dl>{Object.entries(execution.parameters).map(([key, value]) => <div key={key}><dt>{template?.parameters.find(p => p.name === key)?.label ?? key}</dt><dd>{String(value)}</dd></div>)}</dl>}
    <fieldset disabled={busy || !!pending}><label>Instance name<input autoFocus required maxLength={200} value={name} onChange={e => setName(e.target.value)}/></label><label>Instance ID<input required pattern="[a-z0-9_-]+" maxLength={128} placeholder="field-pilot-one" value={id} onChange={e => setId(e.target.value)}/></label></fieldset>
    {error && <p className="am-error" role="alert">{error}</p>}{pending && !busy && <p>The result is uncertain. Retry to recover the same deployment.</p>}
    <div className="am-actions"><button type="button" disabled={busy || !!pending} onClick={close}>Cancel</button><button className="am-primary" disabled={busy || !template || !name.trim() || !id}>{busy ? "Submitting…" : pending ? "Retry deployment" : "Deploy instance"}</button></div>
  </form></dialog>;
}
