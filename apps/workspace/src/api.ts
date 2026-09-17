import { browserSession } from "../../console/web/src/csrf.ts";
import { z } from "zod";
import schema from "./generated/workspace.schema.json" with { type: "json" };
import type { AppOperationView, AgentActivity, AgentDefinition, ChatAgent, Run, Chat, ChatSnapshot, ChatSettings, Invitation, InvitationSummary, Message, Person, SendMessage, WorkspaceBootstrap } from "./generated/workspace.ts";
import type { OperationView, OperationPage, OperationSummary, AnswerOperation, StartOperation, Capability, PersonalEvent } from "./generated/workspace.ts";

export type ConversationSnapshot = ChatSnapshot & { activity: AgentActivity };

type Definitions = { PersonalEvent: PersonalEvent; AppOperationView: AppOperationView; OperationView: OperationView; OperationPage: OperationPage; OperationSummary: OperationSummary; Capability: Capability; AgentActivity: AgentActivity; AgentDefinition: AgentDefinition; ChatAgent: ChatAgent; Run: Run; Chat: Chat; ChatSnapshot: ChatSnapshot; Invitation: Invitation;
  InvitationSummary: InvitationSummary; Message: Message; Person: Person; WorkspaceBootstrap: WorkspaceBootstrap };
const validators = new Map<keyof Definitions, z.ZodType>();
export function parse<K extends keyof Definitions>(kind: K, input: unknown): Definitions[K] {
  let validator = validators.get(kind);
  if (!validator) {
    const definition: object = { $schema: schema.$schema, $defs: schema.$defs, $ref: `#/$defs/${kind}` };
    validator = z.fromJSONSchema(definition as Parameters<typeof z.fromJSONSchema>[0]);
    validators.set(kind, validator);
  }
  // Types and schemas are emitted from the same Rust contract.
  return validator.parse(input) as Definitions[K];
}

export class ApiError extends Error {
  readonly status: number;
  constructor(status: number) {
    super(status === 401 ? "Sign in to continue." : status === 403 || status === 404
      ? "This chat or action is no longer available with your access."
      : status === 409 ? "The chat changed. Refresh its details and try again."
      : status === 429 ? "There are too many active requests. Please try again shortly."
      : "The request could not be confirmed. Please try again.");
    this.status = status;
  }
}
export async function request(path: string, method = "GET", body?: unknown, signal?: AbortSignal): Promise<unknown> {
  if (method !== "GET" && !browserSession.csrfToken) throw new ApiError(401);
  const headers: Record<string, string> = { Accept: "application/json" };
  if (method !== "GET") {
    headers["X-Veoveo-CSRF-Token"] = browserSession.csrfToken!;
    headers["Content-Type"] = "application/json";
  }
  const response = await fetch(`/workspace/api${path}`, {
    method, credentials: "same-origin", headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(15_000)]) : AbortSignal.timeout(15_000),
  });
  browserSession.csrfToken = response.headers.get("x-veoveo-csrf-token") ?? browserSession.csrfToken;
  if (!response.ok) {
    if (response.status === 401) { browserSession.csrfToken = undefined; window.dispatchEvent(new Event("workspace-auth-expired")); }
    throw new ApiError(response.status);
  }
  return response.status === 204 ? undefined : response.json();
}
function list<K extends keyof Definitions>(kind: K, input: unknown): Definitions[K][] {
  if (!Array.isArray(input)) throw new Error("The server returned an invalid list.");
  return input.map(value => parse(kind, value));
}
export const api = {
  session: async (signal?: AbortSignal) => parse("WorkspaceBootstrap", await request("/session", "GET", undefined, signal)),
  chats: async (signal?: AbortSignal) => list("Chat", await request("/chats", "GET", undefined, signal)),
  invitations: async (signal?: AbortSignal) => list("InvitationSummary", await request("/invitations", "GET", undefined, signal)),
  create: async (id: string, title: string) => parse("Chat", await request("/chats", "POST", { id, title })),
  snapshot: async (chat: string, page: { after?: number; before?: number } = {}, signal?: AbortSignal) => {
    const query = new URLSearchParams(Object.entries(page).map(([key, value]) => [key, String(value)]));
    const value = parse("ChatSnapshot", await request(`/chats/${encodeURIComponent(chat)}?${query}`, "GET", undefined, signal));
    if (value.chat.id !== chat) throw new Error("The chat response could not be verified.");
    return { ...value, activity: parse("AgentActivity", await request(`/chats/${encodeURIComponent(chat)}/activity`, "GET", undefined, signal)) } satisfies ConversationSnapshot;
  },
  agents: async (signal?: AbortSignal) => list("AgentDefinition", await request("/agents", "GET", undefined, signal)),
  addAgent: async (chat: string, definition: string) => parse("ChatAgent", await request(`/chats/${chat}/agents`, "POST", { definition })),
  removeAgent: async (chat: string, agent: string) => parse("ChatAgent", await request(`/chats/${chat}/agents/${agent}`, "DELETE")),
  cancelRun: async (chat: string, run: string) => parse("Run", await request(`/chats/${chat}/runs/${run}/cancel`, "POST")),
  operations: async (chat?: string, before?: string, signal?: AbortSignal) => {
    const query = new URLSearchParams(); if (chat) query.set("chat", chat); if (before) query.set("before", before);
    return parse("OperationPage", await request(`/operations?${query}`, "GET", undefined, signal));
  },
  operation: async (id: string, signal?: AbortSignal) => parse("OperationView", await request(`/operations/${id}`, "GET", undefined, signal)),
  startOperation: async (chat: string, value: StartOperation) => parse("OperationSummary", await request(`/chats/${chat}/operations`, "POST", value)),
  cancelOperation: (id: string) => request(`/operations/${id}/cancel`, "POST"),
  answerOperation: (id: string, value: AnswerOperation) => request(`/operations/${id}/input`, "POST", value),
  capabilities: async (signal?: AbortSignal) => list("Capability", await request("/capabilities", "GET", undefined, signal)),
  send: async (chat: string, message: SendMessage) => parse("Message", await request(`/chats/${chat}/messages`, "POST", message)),
  people: async (query: string, signal?: AbortSignal) => list("Person", await request(`/people?q=${encodeURIComponent(query)}`, "GET", undefined, signal)),
  invite: async (chat: string, invitee: string, id: string) => parse("Invitation", await request(`/chats/${chat}/invitations`, "POST", { id, invitee })),
  decide: async (invitation: Invitation, state: "accepted" | "declined") => parse("Invitation", await request(`/invitations/${invitation.id}`, "POST", { chatId: invitation.chatId, state })),
  settings: async (chat: string, value: ChatSettings) => parse("Chat", await request(`/chats/${chat}`, "PUT", value)),
  remove: (chat: string, person: string) => request(`/chats/${chat}/members/${person}`, "DELETE"),
};

export function loginPath(): string {
  return `/workspace/auth/login?${new URLSearchParams({ return_to: `${location.pathname}${location.search}${location.hash}` })}`;
}

export async function logout(): Promise<void> {
  if (!browserSession.csrfToken) return;
  const response = await fetch("/workspace/auth/logout", { method: "POST", credentials: "same-origin", redirect: "manual",
    headers: { "X-Veoveo-CSRF-Token": browserSession.csrfToken } });
  if (!(response.ok || response.type === "opaqueredirect")) throw new ApiError(response.status);
  browserSession.csrfToken = undefined;
  location.replace("/workspace/");
}
