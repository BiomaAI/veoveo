import { templateExecution } from "./content";
import type { Content, TemplateChoice } from "../generated/agent-management";

export function TemplateAuthority({ template }: { template: TemplateChoice }) {
  return <section className="am-authority" aria-label="Managed authority"><h3>Automated authority</h3>
    <p>Each instance receives its own service identity and retained memory. Resource owners control its vehicle and Computer grants.</p>
    <dl><dt>Scopes</dt><dd>{template.scopes.join(", ")}</dd><dt>Roles</dt><dd>{template.roles.join(", ") || "None"}</dd><dt>Context membership</dt><dd>{template.membership}</dd><dt>Retained storage</dt><dd>{template.storageGib} GiB per instance</dd></dl>
  </section>;
}

export function TemplateFields({ templates, value, change }: { templates: TemplateChoice[]; value: Content; change: (value: Content) => void }) {
  const execution = value.execution;
  if (execution.kind !== "managed") return null;
  const template = templates.find(t => t.id === execution.template);
  return <section className="am-parameters"><label>Runtime template<select value={execution.template} onChange={e => {
    const selected = templates.find(t => t.id === e.target.value)!;
    change({ ...value, execution: templateExecution(selected), tools: value.tools.filter(t => selected.tools.includes(t)) });
  }}>{!template && <option value={execution.template}>Unavailable template: {execution.template}</option>}{templates.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}</select></label>
    {template && <>
      {template.revision !== execution.templateRevision && <p className="am-warning">This template changed. <button type="button" onClick={() => change({ ...value, execution: { ...execution, templateRevision: template.revision } })}>Use the approved template</button></p>}
      <p>Template parameters belong to each published revision. Changing parameters requires a new instance to preserve existing memory safely.</p>
      {template.parameters.map(p => <label key={p.name}>{p.label}{p.shape.kind === "choice" ? <select value={String(execution.parameters[p.name] ?? "")} onChange={e => change({ ...value, execution: { ...execution, parameters: { ...execution.parameters, [p.name]: e.target.value } } })}>{p.shape.values.map(v => <option key={v}>{v}</option>)}</select>
        : p.shape.kind === "boolean" ? <input type="checkbox" checked={execution.parameters[p.name] === true} onChange={e => change({ ...value, execution: { ...execution, parameters: { ...execution.parameters, [p.name]: e.target.checked } } })}/>
        : <input required type={p.shape.kind === "integer" ? "number" : "text"} min={p.shape.kind === "integer" ? p.shape.minimum : undefined} max={p.shape.kind === "integer" ? p.shape.maximum : undefined} maxLength={p.shape.kind === "identifier" ? p.shape.maxLength : undefined} pattern={p.shape.kind === "identifier" ? "[A-Za-z0-9_-]+" : undefined} value={String(execution.parameters[p.name] ?? "")} onChange={e => change({ ...value, execution: { ...execution, parameters: { ...execution.parameters, [p.name]: p.shape.kind === "integer" ? Number(e.target.value) : e.target.value } } })}/>}</label>)}
      {template.resourceSubscriptions.length > 0 && <section><h3>Resource notifications</h3><p>Selected resources may wake the agent when their content changes. Idle observation does not call a model.</p>{template.resourceSubscriptions.map(uri => <label key={uri} className="am-check"><input type="checkbox" checked={execution.resourceSubscriptions.includes(uri)} onChange={e => change({ ...value, execution: { ...execution, resourceSubscriptions: e.target.checked ? [...execution.resourceSubscriptions, uri] : execution.resourceSubscriptions.filter(v => v !== uri) } })}/><span>{uri}</span></label>)}</section>}
      <TemplateAuthority template={template}/>
    </>}
  </section>;
}
