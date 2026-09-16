import type { AgentActivity, ChatSnapshot, Message, RunState, RunFailure } from "./generated/workspace.ts";
import type { ThreadMessageLike } from "@assistant-ui/react";

// Per-message presentation retains identity even when several people or agents
// write consecutively. It never determines authorization or run admission.
export type PresentedMessage = {
  id: string; text: string; authorId: string; authorName: string;
  kind: "human" | "agent"; createdAt: string; mine: boolean;
  run?: { id: string; state: RunState; failure?: RunFailure | null; canCancel?: boolean };
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
      mine: message.mine, kind: message.kind, runId: message.run?.id, runState: message.run?.state, runFailure: message.run?.failure, canCancel: message.run?.canCancel } },
  };
}
export function present(snapshot: ChatSnapshot, personId: string, activity?: AgentActivity): PresentedMessage[] {
  const members = new Map(snapshot.members.map(member => [member.id, member]));
  const humans = snapshot.messages.map(message => {
    const member = members.get(message.author);
    return { id: message.id, text: message.text, authorId: message.author,
      authorName: member?.person.displayName ?? "Former participant", kind: "human" as const,
      createdAt: message.createdAt, mine: member?.person.id === personId };
  });
  const bySequence = new Map(snapshot.messages.map(message => [message.id, message.sequence]));
  const agents = (activity?.runs ?? []).map(run => {
    bySequence.set(run.id, run.sequence);
    const agent = activity?.agents.find(agent => agent.id === run.agent);
    return { id: run.id, text: run.text, authorId: run.agent, authorName: agent?.name ?? "Agent",
      kind: "agent" as const, createdAt: run.createdAt, mine: false,
      run: { id: run.id, state: run.state, failure: run.failure, canCancel: run.initiator === personId || snapshot.chat.owner === personId } };
  });
  return [...humans, ...agents].sort((a, b) => (bySequence.get(a.id) ?? 0) - (bySequence.get(b.id) ?? 0));
}
export function mergeMessages(current: Message[], incoming: Message[]): Message[] {
  const byId = new Map(current.map(message => [message.id, message]));
  for (const message of incoming) byId.set(message.id, message);
  return [...byId.values()].sort((a, b) => a.sequence - b.sequence);
}
