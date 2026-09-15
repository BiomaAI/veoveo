import type { ChatSnapshot, Message } from "./generated/workspace.ts";
import type { ThreadMessageLike } from "@assistant-ui/react";

// Per-message presentation retains identity even when several people or agents
// write consecutively. It never determines authorization or run admission.
export type PresentedMessage = {
  id: string; text: string; authorId: string; authorName: string;
  kind: "human" | "agent"; createdAt: string; mine: boolean;
  run?: { id: string; state: "running" | "completed" | "cancelled" | "failed" };
};
export function toThreadMessage(message: PresentedMessage): ThreadMessageLike & { convertConfig: { joinStrategy: "none" } } {
  return {
    convertConfig: { joinStrategy: "none" },
    id: message.id,
    role: message.kind === "agent" ? "assistant" : "user",
    content: [{ type: "text", text: message.text }],
    createdAt: new Date(message.createdAt),
    ...(message.kind === "agent" ? { status: message.run?.state === "running"
      ? { type: "running" as const } : { type: "complete" as const, reason: "stop" as const } } : {}),
    metadata: { custom: { authorId: message.authorId, authorName: message.authorName,
      mine: message.mine, kind: message.kind, runId: message.run?.id, runState: message.run?.state } },
  };
}
export function present(snapshot: ChatSnapshot, personId: string): PresentedMessage[] {
  const members = new Map(snapshot.members.map(member => [member.id, member]));
  return snapshot.messages.map(message => {
    const member = members.get(message.author);
    return { id: message.id, text: message.text, authorId: message.author,
      authorName: member?.person.displayName ?? "Former participant", kind: "human",
      createdAt: message.createdAt, mine: member?.person.id === personId };
  });
}
export function mergeMessages(current: Message[], incoming: Message[]): Message[] {
  const byId = new Map(current.map(message => [message.id, message]));
  for (const message of incoming) byId.set(message.id, message);
  return [...byId.values()].sort((a, b) => a.sequence - b.sequence);
}
