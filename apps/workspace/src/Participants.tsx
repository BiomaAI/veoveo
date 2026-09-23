import { AgentRevisionUpdate } from "./AgentRevisionUpdate.tsx";
import { uuidV7 } from "../../console/web/src/agentControl.ts";
import { useEffect, useRef, useState } from "react";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bot, Check, Search, UserPlus, X } from "lucide-react";
import { api } from "./api.ts";
import { AgentParticipation } from "./AgentParticipation.tsx";
import { initials } from "./identity.ts";
import type { ConversationSnapshot } from "./api.ts";

export function Participants({ snapshot, personId, onChanged, close }: {
  snapshot: ConversationSnapshot; personId: string; onChanged: () => Promise<void>; close: () => void;
}) {
  const { chat, members } = snapshot;
  const owner = chat.owner === personId;
  const [search, setSearch] = useState("");
  const [title, setTitle] = useState(chat.title);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const results = useQuery({ queryKey: ["people", search], queryFn: ({ signal }) => api.people(search, signal), enabled: search.trim().length >= 2 });
  async function act(action: () => Promise<unknown>, notice?: string) {
    setBusy(true); setError(undefined); setNotice(undefined);
    try { await action(); await onChanged(); if (notice) setNotice(notice); }
    catch (error) { setError(error instanceof Error ? error.message : "Veoveo couldn't update this chat. Try again."); }
    finally { setBusy(false); }
  }
  const client = useQueryClient();
  const catalog = useInfiniteQuery({ queryKey: ["agent-catalog"], initialPageParam: undefined as string | undefined,
    queryFn: ({ signal, pageParam }) => api.agents(pageParam, signal), getNextPageParam: page => page.next ?? undefined,
    enabled: owner && !chat.archived });
  const choices = catalog.data?.pages.flatMap(page => page.items) ?? [];
  const addRequests = useRef(new Map<string, string>());
  useEffect(() => {
    if (!owner || chat.archived) return;
    const events = new EventSource("/workspace/api/agent-events");
    const refresh = () => { void client.invalidateQueries({ queryKey:["agent-catalog"] }); };
    events.addEventListener("change", refresh);
    return () => events.close();
  }, [client, owner, chat.archived]);
  const [choice, setChoice] = useState("");
  const candidate = choices.find(agent => agent.id === choice);
  const settings = { expectedRevision: chat.revision, title, archived: chat.archived, membersCanInvite: chat.membersCanInvite, owner: chat.owner, participation: chat.participation };
  return <aside className="details" aria-label="Chat details">
    <div className="details-heading"><h2>Chat details</h2><button className="icon-button" onClick={close} aria-label="Close chat details"><X size={19}/></button></div>
    {error && <p className="error" role="alert">{error}</p>}
    {notice && <p className="notice" role="status"><Check size={14}/>{notice}</p>}
    <section><h3>People · {members.filter(member => member.active).length}</h3>
      {members.filter(member => member.active).map(member => <div className="person" key={member.id}>
        <span className="avatar small">{initials(member.person.displayName)}</span><div><strong>{member.person.displayName}</strong><span>{member.person.id === chat.owner ? "Owner" : "Member"}{member.person.id === personId ? " · You" : ""}</span></div>
        {owner && member.person.id !== personId && <button className="icon-button" aria-label={`Remove ${member.person.displayName}`} disabled={busy}
          onClick={() => void act(() => api.remove(chat.id, member.person.id))}><X size={14}/></button>}
      </div>)}
    </section>
    {(owner || chat.membersCanInvite) && !chat.archived && <section><h3><UserPlus size={15}/> Invite someone</h3>
      <label className="search"><Search size={15}/><input value={search} onChange={event => setSearch(event.target.value)} placeholder="Search people by name" aria-label="Search people" /></label>
      {results.error && <p className="error">{results.error.message}</p>}
      {results.data?.filter(person => !members.some(member => member.active && member.person.id === person.id)).map(person => <div className="person search-result" key={person.id}>
        <span>{person.displayName}</span><button disabled={busy} onClick={() => void act(() => api.invite(chat.id, person.id, crypto.randomUUID()), `Invitation sent to ${person.displayName}.`)}>Invite</button></div>)}
      <p className="muted">People accept an invitation before joining. Members can read the complete shared history.</p>
    </section>}
    <section><h3><Bot size={15}/> Agents · {snapshot.activity.agents.filter(agent => agent.active).length}</h3>
      {snapshot.activity.agents.filter(agent => agent.active).map(agent => <div className="person" key={agent.id}>
        <span className="avatar small agent-avatar"><Bot size={16}/></span><div><strong>{agent.name}</strong><span>{agent.provider} · {agent.model}</span></div>
        {owner && <button className="icon-button" aria-label={`Remove ${agent.name}`} disabled={busy} onClick={() => void act(() => api.removeAgent(chat.id, agent.id))}><X size={14}/></button>}
      </div>)}
      {owner && !chat.archived && snapshot.activity.agents.filter(agent => agent.active).map(agent => <AgentRevisionUpdate key={agent.id} chat={chat.id} agent={agent} latest={choices.find(d => d.id === agent.definition)?.revision} changed={onChanged}/>)}
      {owner && !chat.archived && <>
        {catalog.error && <p className="error">{catalog.error.message}</p>}
        <label>Add an agent<select aria-label="Choose an agent" value={choice} onChange={event => setChoice(event.target.value)}>
          <option value="">Choose an agent…</option>
          {choices.filter(agent => !snapshot.activity.agents.some(member => member.active && member.definition === agent.id)).map(agent => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
        </select></label>
        {catalog.hasNextPage && <button disabled={catalog.isFetchingNextPage} onClick={() => void catalog.fetchNextPage()}>Load more agents</button>}
        {candidate && <div className="agent-disclosure"><p>{candidate.description}</p><p className="muted">{candidate.provider} · {candidate.model}. This agent receives the shared history when asked to respond.</p>
          <p className="muted">{candidate.tools.length ? `Tools: ${candidate.tools.join(", ")}. The agent uses each tool with the permissions of the person who asked, and results appear only in that person's Activity.` : "This agent doesn't use any tools."}</p>
          <button disabled={busy} onClick={() => void act(async () => { const fingerprint = `${candidate.id}:${candidate.revision}`; let request = addRequests.current.get(fingerprint);
            if (!request) { request = uuidV7(); addRequests.current.set(fingerprint, request); }
            await api.addAgent(chat.id, candidate.id, candidate.revision, request); addRequests.current.delete(fingerprint); setChoice(""); })}>Add {candidate.name}</button></div>}
        {!catalog.isPending && choices.length === 0 && <p className="muted">No agents are published in this Work Context yet. Ask an administrator to publish one.</p>}
      </>}
    </section>
    {owner && <section className="settings"><h3>Owner controls</h3>
      <AgentParticipation key={chat.revision} policy={chat.participation} agents={snapshot.activity.agents} busy={busy}
        onSave={participation => void act(() => api.settings(chat.id, { ...settings, title: chat.title, participation }))}/>
      <label>Chat name<input value={title} maxLength={200} onChange={event => setTitle(event.target.value)}/></label>
      <button disabled={busy || !title.trim() || title === chat.title} onClick={() => void act(() => api.settings(chat.id, { ...settings, title: title.trim() }))}>Save name</button>
      <label className="check"><input type="checkbox" checked={chat.membersCanInvite} disabled={busy} onChange={event => void act(() => api.settings(chat.id, { ...settings, title: chat.title, membersCanInvite: event.target.checked }))}/> Members may invite people</label>
      <label>Transfer ownership<select value={chat.owner} disabled={busy} onChange={event => void act(() => api.settings(chat.id, { ...settings, title: chat.title, owner: event.target.value }))}>
        {members.filter(member => member.active).map(member => <option key={member.id} value={member.person.id}>{member.person.displayName}</option>)}
      </select></label>
      <button disabled={busy} onClick={() => void act(() => api.settings(chat.id, { ...settings, title: chat.title, archived: !chat.archived }))}>{chat.archived ? "Reopen chat" : "Archive chat"}</button>
    </section>}
    {!owner && <button className="danger" disabled={busy} onClick={() => void act(() => api.remove(chat.id, personId))}>Leave chat</button>}
  </aside>;
}
