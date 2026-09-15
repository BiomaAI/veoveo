import { ExternalLink } from "lucide-react";
import { formFields, readForm, type FormField } from "./taskForms.ts";
import type { InputAnswer, InputDecision, OperationInput } from "./generated/workspace.ts";

export function FormFields({ fields, prefix }: { fields: FormField[]; prefix: string }) {
  return fields.map((field, index) => {
    const name = `${prefix}-${index}`;
    const label = <>{field.label}{field.required && <span aria-label="required"> *</span>}</>;
    if (field.kind === "boolean") return <label className="check-field" key={name}><input name={name} type="checkbox" defaultChecked={field.default === true}/>{label}</label>;
    if (field.kind === "multi") return <fieldset className="task-choices" key={name}><legend>{label}</legend>{field.choices?.map(choice => <label className="check-field" key={choice.value}><input type="checkbox" name={name} value={choice.value} defaultChecked={Array.isArray(field.default) && field.default.includes(choice.value)}/>{choice.label}</label>)}</fieldset>;
    return <label key={name}>{label}{field.description && <span className="muted">{field.description}</span>}
      {field.kind === "select" ? <select name={name} required={field.required} defaultValue={typeof field.default === "string" ? field.default : ""}><option value="">Choose…</option>{field.choices?.map(choice => <option key={choice.value} value={choice.value}>{choice.label}</option>)}</select>
        : <input name={name} required={field.required} type={field.kind === "number" || field.kind === "integer" ? "number" : field.format === "email" ? "email" : field.format === "uri" ? "url" : field.format === "date" ? "date" : "text"}
          step={field.kind === "integer" ? 1 : field.kind === "number" ? "any" : undefined}
          min={field.minimum} max={field.maximum} minLength={field.minLength} maxLength={field.maxLength}
          defaultValue={typeof field.default === "string" || typeof field.default === "number" ? field.default : undefined}/>}</label>;
  });
}

export function TaskInput({ inputs, busy, onAnswer, onError }: {
  inputs: OperationInput[]; busy: boolean; onAnswer: (answers: InputAnswer[]) => void; onError: (message: string) => void;
}) {
  let fields: FormField[][];
  try { fields = inputs.map(input => input.kind === "form" ? formFields(input.schema) : []); }
  catch { return <p className="error">This form uses fields the client cannot display. You can decline the request or cancel the task.<button disabled={busy} onClick={() => onAnswer(inputs.map(input => ({ id: input.id, digest: input.digest, decision: "decline", content: null })))}>Decline request</button></p>; }
  const unsupported = inputs.some(input => input.kind === "unsupported");
  function submit(form: HTMLFormElement, decision: InputDecision) {
    try {
      const data = new FormData(form);
      onAnswer(inputs.map((input, index) => ({ id: input.id, digest: input.digest, decision,
        content: decision === "accept" && input.kind === "form" ? readForm(fields[index]!, data, `input-${index}`) : null })));
    } catch (error) { onError(error instanceof Error ? error.message : "Check the form values."); }
  }
  return <form className="task-input" onSubmit={event => { event.preventDefault(); submit(event.currentTarget, "accept"); }}>
    <fieldset disabled={busy}>{inputs.map((input, index) => <section key={`${input.id}:${input.digest}`}>
      <p>{input.message}</p>
      {input.kind === "form" && <FormFields fields={fields[index]!} prefix={`input-${index}`}/>}
      {input.kind === "link" && input.url && <a href={input.url} target="_blank" rel="noopener noreferrer">Open requested page <ExternalLink size={13}/></a>}
    </section>)}
    {!unsupported && <div className="actions"><button className="primary" type="submit">{busy ? "Submitting…" : "Continue"}</button><button type="button" onClick={event => { const form = event.currentTarget.form; if (form) submit(form, "decline"); }}>Decline</button></div>}
    </fieldset>
  </form>;
}
