import { useState } from "react";
import type { ChatAgent, Participation, ParticipationMode } from "./generated/workspace.ts";

export function AgentParticipation({ policy, agents, busy, onSave }: {
  policy: Participation; agents: ChatAgent[]; busy: boolean; onSave: (policy: Participation) => void;
}) {
  const [draft, setDraft] = useState(policy);
  const active = agents.filter(agent => agent.active);
  const valid = draft.mode === "on_request" ? draft.agents.length === 0 : draft.mode === "default" ? draft.agents.length === 1 : draft.agents.length > 0 && draft.agents.length <= 4;
  return <fieldset className="participation-settings" disabled={busy}>
    <legend>Agent responses</legend>
    <label>Participation<select aria-label="Agent participation" value={draft.mode} onChange={event => {
      const mode = event.target.value as ParticipationMode;
      setDraft({ mode, agents: mode === "on_request" ? [] : mode === "default" ? draft.agents.slice(0, 1) : draft.agents });
    }}>
      <option value="on_request">When someone asks</option>
      <option value="default">Default assistant</option>
      <option value="automatic">Automatic responses</option>
    </select></label>
    {draft.mode !== "on_request" && active.map(agent => <label className="check" key={agent.id}>
      <input type={draft.mode === "default" ? "radio" : "checkbox"} name="response-agents" checked={draft.agents.includes(agent.id)}
        disabled={!draft.agents.includes(agent.id) && draft.mode === "automatic" && draft.agents.length >= 4}
        onChange={event => setDraft(current => ({ ...current, agents: current.mode === "default" ? [agent.id]
          : event.target.checked ? [...current.agents, agent.id] : current.agents.filter(id => id !== agent.id) }))}/>{agent.name}
    </label>)}
    <p className="muted">{draft.mode === "on_request" ? "People select agents or begin a message with @Name to request a response."
      : draft.mode === "default" ? "This assistant responds when a human message addresses no other agent."
      : "These agents respond to new human messages. Up to four may respond to each message."} Agent replies never trigger another response. Agents use tools with the permissions of the person who sent the message.</p>
    <button disabled={!valid || JSON.stringify(draft) === JSON.stringify(policy)} onClick={() => onSave(draft)}>Save participation</button>
  </fieldset>;
}
