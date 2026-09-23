import type { AgentCatalogPage, AgentRevisionPreview, UpdateChatAgent } from "./generated/workspace.ts";
import { browserSession } from "../../console/web/src/csrf.ts";
import { z } from "zod";
import schema from "./generated/workspace.schema.json" with { type: "json" };
import type { AppOperationView, AgentActivity, AgentDefinition, ChatAgent, Run, Chat, ChatSnapshot, ChatSettings, Invitation, InvitationSummary, Message, Person, SendMessage, WorkspaceBootstrap } from "./generated/workspace.ts";
import type { OperationView, OperationPage, OperationSummary, AnswerOperation, StartOperation, Capability, PersonalEvent } from "./generated/workspace.ts";

export type ConversationSnapshot = ChatSnapshot & { activity: AgentActivity };

type Definitions = { AgentCatalogPage: AgentCatalogPage; AgentRevisionPreview: AgentRevisionPreview; PersonalEvent: PersonalEvent; AppOperationView: AppOperationView; OperationView: OperationView; OperationPage: OperationPage; OperationSummary: OperationSummary; Capability: Capability; AgentActivity: AgentActivity; AgentDefinition: AgentDefinition; ChatAgent: ChatAgent; Run: Run; Chat: Chat; ChatSnapshot: ChatSnapshot; Invitation: Invitation;
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
  const result = validator.safeParse(input);
  if (!result.success) {
    console.error(`Workspace could not read ${kind}`, result.error);
    throw new Error(unreadableResponse);
  }
  return result.data as Definitions[K];
}

export const unreadableResponse = "Veoveo returned data this page couldn't read. Reload; if it continues, tell your administrator.";

/** Browser-facing text for a failed request. `action` completes "couldn't …". */
export function statusMessage(status: number, action = "complete this request"): string {
  switch (status) {
    case 0: return `Veoveo couldn't ${action}. Check your connection and try again.`;
    case 401: return "Your session ended. Sign in again to continue.";
    case 403: return `You don't have permission to ${action}. Ask the chat owner or an administrator for access.`;
    case 404: return "This item no longer exists, or you no longer have access to it.";
    case 409: return "This changed while you were working. Refresh and try again.";
    case 413: return "This is too large to send.";
    case 429: return "Too many requests are running. Try again in a moment.";
    default: return status >= 500 ? `Veoveo couldn't ${action}. Try again in a moment.` : `Veoveo couldn't ${action}. Check your input and try again.`;
  }
}

export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, action?: string) {
    super(statusMessage(status, action));
    this.status = status;
  }
}
export async function request(path: string, method = "GET", body?: unknown, signal?: AbortSignal, action?: string): Promise<unknown> {
  if (method !== "GET" && !browserSession.csrfToken) throw new ApiError(401, action);
  const headers: Record<string, string> = { Accept: "application/json" };
  if (method !== "GET") {
    headers["X-Veoveo-CSRF-Token"] = browserSession.csrfToken!;
    headers["Content-Type"] = "application/json";
  }
  let response: Response;
  try {
    response = await fetch(`/workspace/api${path}`, {
      method, credentials: "same-origin", headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(15_000)]) : AbortSignal.timeout(15_000),
    });
  } catch (error) {
    // A caller's own cancellation keeps its abort reason; a network failure or
    // timeout becomes a message the person can act on.
    if (signal?.aborted) throw error;
    throw new ApiError(0, action);
  }
  browserSession.csrfToken = response.headers.get("x-veoveo-csrf-token") ?? browserSession.csrfToken;
  if (!response.ok) {
    if (response.status === 401) { browserSession.csrfToken = undefined; window.dispatchEvent(new Event("workspace-auth-expired")); }
    throw new ApiError(response.status, action);
  }
  if (response.status === 204) return undefined;
  try { return await response.json(); }
  catch { throw new Error(unreadableResponse); }
}
function list<K extends keyof Definitions>(kind: K, input: unknown): Definitions[K][] {
  if (!Array.isArray(input)) throw new Error(unreadableResponse);
  return input.map(value => parse(kind, value));
}
export const api = {
  session: async (signal?: AbortSignal) => parse("WorkspaceBootstrap", await request("/session", "GET", undefined, signal, "open your workspace")),
  chats: async (signal?: AbortSignal) => list("Chat", await request("/chats", "GET", undefined, signal, "load your chats")),
  invitations: async (signal?: AbortSignal) => list("InvitationSummary", await request("/invitations", "GET", undefined, signal, "load your invitations")),
  create: async (id: string, title: string) => parse("Chat", await request("/chats", "POST", { id, title }, undefined, "create the chat")),
  snapshot: async (chat: string, page: { after?: number; before?: number } = {}, signal?: AbortSignal) => {
    const query = new URLSearchParams(Object.entries(page).map(([key, value]) => [key, String(value)]));
    const value = parse("ChatSnapshot", await request(`/chats/${encodeURIComponent(chat)}?${query}`, "GET", undefined, signal, "load this chat"));
    if (value.chat.id !== chat) throw new Error(unreadableResponse);
    return { ...value, activity: parse("AgentActivity", await request(`/chats/${encodeURIComponent(chat)}/activity`, "GET", undefined, signal, "load this chat")) } satisfies ConversationSnapshot;
  },
  agents: async (after?: string, signal?: AbortSignal) => parse("AgentCatalogPage", await request(`/agents${after ? `?after=${encodeURIComponent(after)}` : ""}`, "GET", undefined, signal, "load agents")),
  addAgent: async (chat: string, definition: string, revision: string, requestId: string) => parse("ChatAgent", await request(`/chats/${chat}/agents`, "POST", { definition, revision, requestId }, undefined, "add the agent")),
  agentRevision: async (chat: string, agent: string) => parse("AgentRevisionPreview", await request(`/chats/${chat}/agents/${agent}/revision`, "GET", undefined, undefined, "load the agent update")),
  updateAgent: async (chat: string, agent: string, body: UpdateChatAgent) => parse("ChatAgent", await request(`/chats/${chat}/agents/${agent}/revision`, "POST", body, undefined, "update the agent")),
  removeAgent: async (chat: string, agent: string) => parse("ChatAgent", await request(`/chats/${chat}/agents/${agent}`, "DELETE", undefined, undefined, "remove the agent")),
  cancelRun: async (chat: string, run: string) => parse("Run", await request(`/chats/${chat}/runs/${run}/cancel`, "POST", undefined, undefined, "stop the response")),
  operations: async (chat?: string, before?: string, signal?: AbortSignal) => {
    const query = new URLSearchParams(); if (chat) query.set("chat", chat); if (before) query.set("before", before);
    return parse("OperationPage", await request(`/operations?${query}`, "GET", undefined, signal, "load your activity"));
  },
  operation: async (id: string, signal?: AbortSignal) => parse("OperationView", await request(`/operations/${id}`, "GET", undefined, signal, "load this task")),
  startOperation: async (chat: string, value: StartOperation) => parse("OperationSummary", await request(`/chats/${chat}/operations`, "POST", value, undefined, "start the task")),
  cancelOperation: (id: string) => request(`/operations/${id}/cancel`, "POST", undefined, undefined, "cancel the task"),
  answerOperation: (id: string, value: AnswerOperation) => request(`/operations/${id}/input`, "POST", value, undefined, "send your answer"),
  capabilities: async (signal?: AbortSignal) => list("Capability", await request("/capabilities", "GET", undefined, signal, "load tools")),
  send: async (chat: string, message: SendMessage) => parse("Message", await request(`/chats/${chat}/messages`, "POST", message, undefined, "send your message")),
  people: async (query: string, signal?: AbortSignal) => list("Person", await request(`/people?q=${encodeURIComponent(query)}`, "GET", undefined, signal, "search for people")),
  invite: async (chat: string, invitee: string, id: string) => parse("Invitation", await request(`/chats/${chat}/invitations`, "POST", { id, invitee }, undefined, "invite this person")),
  decide: async (invitation: Invitation, state: "accepted" | "declined") => parse("Invitation", await request(`/invitations/${invitation.id}`, "POST", { chatId: invitation.chatId, state }, undefined, "update the invitation")),
  settings: async (chat: string, value: ChatSettings) => parse("Chat", await request(`/chats/${chat}`, "PUT", value, undefined, "update this chat")),
  remove: (chat: string, person: string) => request(`/chats/${chat}/members/${person}`, "DELETE", undefined, undefined, "remove this person"),
};

export function loginPath(): string {
  return `/workspace/auth/login?${new URLSearchParams({ return_to: `${location.pathname}${location.search}${location.hash}` })}`;
}

export async function logout(): Promise<void> {
  if (!browserSession.csrfToken) return;
  const response = await fetch("/workspace/auth/logout", { method: "POST", credentials: "same-origin", redirect: "manual",
    headers: { "X-Veoveo-CSRF-Token": browserSession.csrfToken } });
  if (!(response.ok || response.type === "opaqueredirect")) throw new ApiError(response.status, "sign you out");
  browserSession.csrfToken = undefined;
  location.replace("/workspace/");
}
