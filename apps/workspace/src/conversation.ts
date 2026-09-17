import type { AgentActivity, ChatAttachment, ChatSnapshot, Message, ReplyContext, ReplyTarget, RunState, RunFailure, RunFeedback } from "./generated/workspace.ts";
import type { ThreadMessageLike } from "@assistant-ui/react";

// Per-message presentation retains identity even when several people or agents
// write consecutively. It never determines authorization or run admission.
export type PresentedMessage = {
  id: string; text: string; attachments: ChatAttachment[]; authorId: string; authorName: string;
  kind: "human" | "agent"; createdAt: string; mine: boolean;
  target: ReplyTarget; replyTo?: ReplyTarget | null; replyContext?: ReplyContext | null;
  run?: { id: string; state: RunState; feedback: RunFeedback; failure?: RunFailure | null; canCancel?: boolean };
};
export function toThreadMessage(message: PresentedMessage): ThreadMessageLike & { convertConfig: { joinStrategy: "none" } } {
  return {
    convertConfig: { joinStrategy: "none" },
    id: message.id,
    role: message.kind === "agent" ? "assistant" : "user",
    content: [{ type: "text", text: message.text }],
    createdAt: new Date(message.createdAt),
    ...(message.kind === "agent" ? { status: (message.run?.state === "running" || message.run?.state === "queued")
      ? { type: "running" as const } : { type: "complete" as const, reason: "stop" as const } } : {}),
    metadata: { custom: { authorId: message.authorId, authorName: message.authorName,
      mine: message.mine, kind: message.kind, runId: message.run?.id, runState: message.run?.state, runPhase: message.run?.feedback.phase, runOperations: message.run?.feedback.operations, runFailure: message.run?.failure, canCancel: message.run?.canCancel } },
  };
}
export function present(snapshot: ChatSnapshot, personId: string, activity?: AgentActivity): PresentedMessage[] {
  const members = new Map(snapshot.members.map(member => [member.id, member]));
  const humans = snapshot.messages.map(message => {
    const member = members.get(message.author);
    return { id: `message:${message.id}`, target: { kind: "message" as const, id: message.id }, text: message.text, authorId: message.author,
      attachments: message.attachments, replyTo: message.replyTo, replyContext: message.replyContext,
      authorName: member?.person.displayName ?? "Former participant", kind: "human" as const,
      createdAt: message.createdAt, mine: member?.person.id === personId };
  });
  const bySequence = new Map(snapshot.messages.map(message => [`message:${message.id}`, message.sequence]));
  const agents = (activity?.runs ?? []).map(run => {
    bySequence.set(`response:${run.id}`, run.sequence);
    const agent = activity?.agents.find(agent => agent.id === run.agent);
    return { id: `response:${run.id}`, target: { kind: "response" as const, id: run.id }, replyTo: { kind: "message" as const, id: run.trigger }, text: run.text, authorId: run.agent, authorName: agent?.name ?? "Agent",
      attachments: [], kind: "agent" as const, createdAt: run.createdAt, mine: false,
      run: { id: run.id, state: run.state, feedback: run.feedback, failure: run.failure, canCancel: run.initiator === personId || snapshot.chat.owner === personId } };
  });
  return [...humans, ...agents].sort((a, b) => (bySequence.get(a.id) ?? 0) - (bySequence.get(b.id) ?? 0));
}

export function quote(message: PresentedMessage, messages: PresentedMessage[]): ReplyContext | undefined {
  if (!message.replyTo) return undefined;
  if (message.replyContext) return message.replyContext;
  const target = messages.find(item => item.target.kind === message.replyTo!.kind && item.target.id === message.replyTo!.id);
  return target ? { authorName: target.authorName, text: [...target.text].slice(0, 500).join("") }
    : { authorName: message.replyTo.kind === "message" ? "Earlier message" : "Earlier response", text: "The original is outside the loaded history." };
}
export function mergeMessages(current: Message[], incoming: Message[]): Message[] {
  const byId = new Map(current.map(message => [message.id, message]));
  for (const message of incoming) byId.set(message.id, message);
  return [...byId.values()].sort((a, b) => a.sequence - b.sequence);
}
