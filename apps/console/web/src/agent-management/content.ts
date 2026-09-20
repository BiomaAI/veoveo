import type { Authoring, Content, TemplateChoice } from "../generated/agent-management";

export function templateExecution(template: TemplateChoice): Content["execution"] {
  return { kind: "managed", template: template.id, templateRevision: template.revision,
    parameters: Object.fromEntries(template.parameters.map(p => [p.name, p.shape.kind === "identifier" ? "" : p.shape.kind === "choice" ? p.shape.values[0] : p.shape.kind === "integer" ? p.shape.minimum : false])), resourceSubscriptions: [] };
}

export function initialContent(authoring: Authoring, template?: TemplateChoice): Content {
  const model = authoring.models.find(m => !template || template.models.includes(m.reference.id));
  if (!model) throw new Error("An operator must admit a model connection before you can create an agent.");
  return { model: model.reference, instructions: template ? "Complete assigned work within your admitted capabilities and report the result." : "Help the people in this conversation accomplish their requested work.", tools: [], execution: template ? templateExecution(template) : { kind: "chat" },
    budgets: { maxOutputTokens: Math.min(4096, model.limits.maxOutputTokens), maxCompletionCalls: Math.min(4, model.limits.maxCompletionCalls), maxToolCalls: Math.min(8, model.limits.maxToolCalls), deadlineSeconds: Math.min(120, model.limits.deadlineSeconds) } };
}
