import type { ChatAgent, Participation } from "./generated/workspace.ts";

// Only leading addresses request a response. Quoted text, code and ordinary
// mentions in the body cannot silently turn a shared message into agent work.
export function addressedAgents(text: string, selected: string[], agents: ChatAgent[]): string[] {
  const active = agents.filter(agent => agent.active);
  if (selected.some(id => !active.some(agent => agent.id === id))) throw new Error("An addressed agent left this chat. Update your selection.");
  const result = new Set(selected);
  let remaining = text.trimStart();
  while (remaining.startsWith("@")) {
    const matches = active.filter(agent => remaining.startsWith(`@${agent.name}`) && /^(?:\s|[:,]|$)/u.test(remaining.slice(agent.name.length + 1)));
    if (matches.length === 0) break;
    matches.sort((a, b) => b.name.length - a.name.length);
    const agent = matches[0];
    if (matches.filter(match => match.name === agent.name).length > 1) throw new Error(`Several agents are named ${agent.name}. Use the agent selection instead.`);
    result.add(agent.id);
    remaining = remaining.slice(agent.name.length + 1).replace(/^[\s,:]+/u, "");
  }
  if (result.size > 4) throw new Error("Ask up to four agents in one message.");
  return [...result].sort();
}
export function responseAgents(policy: Participation, addressed: string[]): string[] {
  const result = policy.mode === "automatic" ? [...new Set([...addressed, ...policy.agents])]
    : policy.mode === "default" && addressed.length === 0 ? policy.agents : addressed;
  if (result.length > 4) throw new Error("The owner's automatic responses and your selections exceed four agents. Reduce your selection.");
  return result;
}
