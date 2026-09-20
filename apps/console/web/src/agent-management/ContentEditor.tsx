import { useState } from "react";
import type { Authoring, Budgets, CapabilityChoice, Content, TemplateChoice } from "../generated/agent-management";

import { TemplateFields } from "./TemplateFields";

export function ContentEditor({ authoring, capabilities, templates, value, change, disabled }: {
  authoring: Authoring; templates: TemplateChoice[]; capabilities?: CapabilityChoice[]; value: Content; change: (value: Content) => void; disabled: boolean;
}) {
  const [search, setSearch] = useState("");
  const execution = value.execution;
  const template = execution.kind === "managed" ? templates.find(t => t.id === execution.template) : undefined;
  const models = authoring.models.filter(m => !template || template.models.includes(m.reference.id));
  const model = models.find(m => m.reference.id === value.model.id);
  const budgets: [keyof Budgets, string][] = [["maxOutputTokens", "Output tokens"], ["maxCompletionCalls", "Model calls"], ["maxToolCalls", "Tool calls"], ["deadlineSeconds", "Time limit (seconds)"]];
  return <fieldset disabled={disabled} className="am-content">
    <TemplateFields templates={templates} value={value} change={change}/>
    <label>Model<select value={value.model.id} onChange={event => {
      const next = models.find(m => m.reference.id === event.target.value)!;
      const bounded = { ...value.budgets };
      for (const [key] of budgets) bounded[key] = Math.min(bounded[key], next.limits[key]);
      change({ ...value, model: next.reference, budgets: bounded });
    }}>{!model && <option value={value.model.id}>Unavailable model: {value.model.id}</option>}{models.map(m => <option key={m.reference.id} value={m.reference.id}>{m.name} · {m.provider}</option>)}</select></label>
    {model && model.reference.revision !== value.model.revision && <p className="am-warning">This connection changed. <button type="button" onClick={() => change({ ...value, model: model.reference })}>Use the approved revision</button></p>}
    <label>Instructions<textarea rows={8} maxLength={16384} value={value.instructions} onChange={e => change({ ...value, instructions: e.target.value })}/></label>
    <div className="am-budgets">{budgets.map(([key, label]) => <label key={key}>{label}<input type="number" min={key === "maxToolCalls" ? 0 : 1} max={model?.limits[key]} value={value.budgets[key]} onChange={e => change({ ...value, budgets: { ...value.budgets, [key]: Number(e.target.value) } })}/></label>)}</div>
    <div><h3>Capabilities</h3><p>{execution.kind === "managed" ? "Tools use this instance's admitted service authority." : "Tools use the current caller's permissions."} Choosing Computers does not grant access to a Computer.</p>
      <input type="search" aria-label="Filter capabilities" placeholder="Find a capability" value={search} onChange={e => setSearch(e.target.value)}/>
      {capabilities === undefined ? <p role="status">Load capabilities to choose tools.</p> : <div className="am-tools">{capabilities.filter(t => (!template || template.tools.includes(t.name)) && `${t.title} ${t.name}`.toLowerCase().includes(search.toLowerCase())).map(t => <label className="am-check" key={t.name}><input type="checkbox" checked={value.tools.includes(t.name)} disabled={!value.tools.includes(t.name) && value.tools.length >= 64} onChange={e => change({ ...value, tools: e.target.checked ? [...value.tools, t.name] : value.tools.filter(n => n !== t.name) })}/><span>{t.title}<small>{t.name}</small></span></label>)}</div>}
      {value.tools.filter(name => !capabilities?.some(t => t.name === name)).map(name => <div className="am-check" key={name}><span>{name} <small>Availability has not been confirmed.</small></span><button type="button" onClick={() => change({ ...value, tools: value.tools.filter(t => t !== name) })}>Remove</button></div>)}
    </div>
  </fieldset>;
}
