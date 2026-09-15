import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Check, Search, UserPlus, X } from "lucide-react";
import { api } from "./api.ts";
import { initials } from "./identity.ts";
import type { ChatSnapshot } from "./generated/workspace.ts";

export function Participants({ snapshot, personId, onChanged, close }: {
  snapshot: ChatSnapshot; personId: string; onChanged: () => Promise<void>; close: () => void;
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
    catch (error) { setError(error instanceof Error ? error.message : "Could not update this chat."); }
    finally { setBusy(false); }
  }
  const settings = { expectedRevision: chat.revision, title, archived: chat.archived, membersCanInvite: chat.membersCanInvite, owner: chat.owner };
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
    {owner && <section className="settings"><h3>Owner controls</h3>
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
