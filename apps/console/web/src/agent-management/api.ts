import { z } from "zod";
import schema from "../generated/agent-management.schema.json" with { type: "json" };
import type { Authoring, CapabilityChoice, Definition, DefinitionPage, Draft, RevisionPage, Validation, CreateDefinition, SaveDraft, UpdateMetadata, PublishDefinition, RevisionRequest, ValidateDefinition } from "../generated/agent-management";
import { browserSession } from "../csrf.ts";

interface Responses { Authoring: Authoring; CapabilityChoice: CapabilityChoice; Definition: Definition; DefinitionPage: DefinitionPage; Draft: Draft; RevisionPage: RevisionPage; Validation: Validation }
const validators = new Map<keyof Responses, z.ZodType>();
function parse<K extends keyof Responses>(kind: K, value: unknown): Responses[K] {
  let validator = validators.get(kind);
  if (!validator) {
    const definition: object = { $schema: schema.$schema, $defs: schema.$defs, $ref: `#/$defs/${kind}` };
    validator = z.fromJSONSchema(definition as Parameters<typeof z.fromJSONSchema>[0]);
    validators.set(kind, validator);
  }
  return validator.parse(value) as Responses[K];
}
export class AgentApiError extends Error {
  readonly status: number;
  readonly validation?: Validation;
  constructor(status: number, validation?: Validation) {
    super(validation ? validation.findings.map(f => f.message).join(" ")
      : status === 401 ? "Sign in to manage agents."
      : status === 403 ? "Your current access does not permit this action."
      : status === 404 ? "This agent is no longer available in your current Work Context."
      : status === 409 ? "The agent changed. Reload its current version before applying your changes."
      : status === 429 ? "The current capacity limit has been reached."
      : "The result could not be confirmed. Retry to recover the same request.");
    this.status = status; this.validation = validation;
  }
}
export class AgentApi {
  readonly root: string;
  readonly app: "console" | "workspace";
  constructor(app: "console" | "workspace") { this.app = app; this.root = `/${app}/api`; }
  private async request(path: string, method = "GET", body?: unknown, signal?: AbortSignal): Promise<unknown> {
    if (method !== "GET" && !browserSession.csrfToken) throw new AgentApiError(401);
    const response = await fetch(`${this.root}/${path}`, {
      credentials: "same-origin", method,
      headers: { Accept: "application/json", ...(method === "GET" ? {} : { "Content-Type": "application/json", "X-Veoveo-CSRF-Token": browserSession.csrfToken! }) },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(35_000)]) : AbortSignal.timeout(35_000),
    });
    browserSession.csrfToken = response.headers.get("x-veoveo-csrf-token") ?? browserSession.csrfToken;
    if (!response.ok) {
      if (response.status === 401) {
        browserSession.csrfToken = undefined;
        window.dispatchEvent(new Event(`${this.app}-auth-expired`));
      }
      throw new AgentApiError(response.status, response.status === 422 ? parse("Validation", await response.json()) : undefined);
    }
    return response.json();
  }
  authoring = async (signal?: AbortSignal) => parse("Authoring", await this.request("agent-authoring", "GET", undefined, signal));
  list = async (after?: string, signal?: AbortSignal) => parse("DefinitionPage", await this.request(`agent-definitions?limit=50${after ? `&after=${encodeURIComponent(after)}` : ""}`, "GET", undefined, signal));
  read = async (id: string) => parse("Definition", await this.request(`agent-definitions/${encodeURIComponent(id)}`));
  draft = async (id: string) => parse("Draft", await this.request(`agent-definitions/${encodeURIComponent(id)}/draft`));
  revisions = async (id: string, after?: string) => parse("RevisionPage", await this.request(`agent-definitions/${encodeURIComponent(id)}/revisions?limit=20${after ? `&after=${encodeURIComponent(after)}` : ""}`));
  capabilities = async () => {
    const values = await this.request("agent-capabilities");
    if (!Array.isArray(values)) throw new Error("The capability list could not be verified.");
    return values.map(v => parse("CapabilityChoice", v));
  };
  create = async (body: CreateDefinition) => parse("Definition", await this.request("agent-definitions", "POST", body));
  save = async (id: string, body: SaveDraft) => parse("Definition", await this.request(`agent-definitions/${encodeURIComponent(id)}/draft`, "PUT", body));
  metadata = async (id: string, body: UpdateMetadata) => parse("Definition", await this.request(`agent-definitions/${encodeURIComponent(id)}`, "PATCH", body));
  validate = async (id: string, body: ValidateDefinition) => parse("Validation", await this.request(`agent-definitions/${encodeURIComponent(id)}/validate`, "POST", body));
  publish = async (id: string, body: PublishDefinition) => parse("Definition", await this.request(`agent-definitions/${encodeURIComponent(id)}/publish`, "POST", body));
  status = async (id: string, action: "enable" | "disable" | "archive", body: RevisionRequest) => parse("Definition", await this.request(`agent-definitions/${encodeURIComponent(id)}/${action}`, "POST", body));
  observe(changed: () => void): () => void {
    const source = new EventSource(`${this.root}/agent-events`);
    source.addEventListener("change", changed);
    source.addEventListener("expired", changed);
    return () => source.close();
  }
}
