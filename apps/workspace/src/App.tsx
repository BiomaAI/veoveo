import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Activity as ActivityIcon, ArrowRight, Bell, LogOut, Menu, MessageSquare, Monitor, Plus, Upload, Users, X } from "lucide-react";
import { api, ApiError, loginPath, logout } from "./api.ts";
import { initials } from "./identity.ts";
import { useConversation } from "./useConversation.ts";
import { useUploads } from "./useUploads.ts";
import type { Invitation, WorkspaceBootstrap } from "./generated/workspace.ts";

const Conversation = lazy(() => import("./Conversation.tsx").then(module => ({ default: module.Conversation })));
const Activity = lazy(() => import("./Activity.tsx").then(module => ({ default: module.Activity })));
const Participants = lazy(() => import("./Participants.tsx").then(module => ({ default: module.Participants })));
const Computers = lazy(() => import("./Computers.tsx").then(module => ({ default: module.Computers })));
const Uploads = lazy(() => import("./Uploads.tsx").then(module => ({ default: module.Uploads })));

export default function App() {
  const client = useQueryClient();
  const [expired, setExpired] = useState(false);
  const session = useQuery({ queryKey: ["session"], queryFn: ({ signal }) => api.session(signal) });
  useEffect(() => {
    const onExpired = () => { setExpired(true); client.clear(); };
    window.addEventListener("workspace-auth-expired", onExpired);
    return () => window.removeEventListener("workspace-auth-expired", onExpired);
  }, [client]);
  if (expired) return <main className="entry"><div className="wordmark">veoveo<span>Workspace</span></div><h1>Sign in to your workspace.</h1><a className="primary" href={loginPath()}>Sign in <ArrowRight size={17}/></a></main>;
  if (session.isPending) return <main className="entry"><div className="wordmark">veoveo<span>Workspace</span></div><p role="status">Opening your workspace…</p></main>;
  if (!session.data) return <main className="entry"><div className="wordmark">veoveo<span>Workspace</span></div><h1>Good work happens together.</h1><p>Your people, agents, and tools. One place to move work forward.</p>
    {session.error instanceof ApiError && session.error.status === 401 ? <a className="primary" href={loginPath()}>Sign in <ArrowRight size={17}/></a> : <><p className="error" role="alert">{session.error?.message}</p><button onClick={() => void session.refetch()}>Try again</button></>}
  </main>;
  return <Workspace session={session.data}/>;
}

function selectedChat(): string | undefined {
  const value = new URLSearchParams(location.search).get("chat");
  return value && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value) ? value : undefined;
}
function Workspace({ session }: { session: WorkspaceBootstrap }) {
  const client = useQueryClient();
  const uploads = useUploads(session);
  const [uploadsOpen, setUploadsOpen] = useState(false);
  const [selected, setSelected] = useState(selectedChat);
  const [newChat, setNewChat] = useState(false);
  const [inbox, setInbox] = useState(false);
  const [activity, setActivity] = useState(() => new URLSearchParams(location.search).get("view") === "activity");
  const [computers, setComputers] = useState(() => new URLSearchParams(location.search).get("view") === "computers");
  const [mobileNav, setMobileNav] = useState(false);
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const createId = useRef<string | undefined>(undefined);
  const chats = useQuery({ queryKey: ["chats"], queryFn: ({ signal }) => api.chats(signal) });
  const invitations = useQuery({ queryKey: ["invitations"], queryFn: ({ signal }) => api.invitations(signal), refetchOnWindowFocus: true });
  const changed = useCallback(() => { void client.invalidateQueries({ queryKey: ["chats"] }); }, [client]);
  function select(id?: string) {
    setSelected(id); setInbox(false); setActivity(false); setComputers(false); setMobileNav(false);
    history.pushState(null, "", id ? `/workspace/?chat=${id}` : "/workspace/");
  }
  useEffect(() => { const pop = () => { setSelected(selectedChat()); setComputers(new URLSearchParams(location.search).get("view") === "computers"); setActivity(new URLSearchParams(location.search).get("view") === "activity"); setInbox(false); }; window.addEventListener("popstate", pop); return () => window.removeEventListener("popstate", pop); }, []);
  async function create() {
    if (!title.trim() || busy) return;
    createId.current ??= crypto.randomUUID();
    setBusy(true); setError(undefined);
    try { const chat = await api.create(createId.current, title.trim()); select(chat.id); setNewChat(false); setTitle(""); createId.current = undefined; changed(); }
    catch (error) {
      if (error instanceof ApiError && [400, 401, 403, 404, 422].includes(error.status)) createId.current = undefined;
      setError(error instanceof Error ? error.message : "Could not create the chat.");
    }
    finally { setBusy(false); }
  }
  async function decide(invitation: Invitation, state: "accepted" | "declined") {
    setBusy(true); setError(undefined);
    try { await api.decide(invitation, state); await client.invalidateQueries({ queryKey: ["invitations"] }); changed(); if (state === "accepted") select(invitation.chatId); }
    catch (error) { setError(error instanceof Error ? error.message : "Could not update the invitation."); }
    finally { setBusy(false); }
  }
  return <div className={`workspace ${mobileNav ? "show-nav" : ""}`}>
    <aside className="sidebar"><button className="wordmark" onClick={() => select()}>veoveo<span>Workspace</span></button>
      <div className="context-label">{session.tenantName}<span>{session.workContextTitle}</span></div>
      <button className="new-chat" disabled={!session.canContribute} onClick={() => { setNewChat(true); setError(undefined); }}><Plus size={17}/> New chat</button>
      <button className={`nav-item ${inbox ? "active" : ""}`} onClick={() => { setInbox(true); setComputers(false); setActivity(false); setMobileNav(false); void invitations.refetch(); }}><Bell size={16}/> Invitations {!!invitations.data?.length && <span className="count">{invitations.data.length}</span>}</button>
      <button className={`nav-item ${activity ? "active" : ""}`} onClick={() => { setActivity(true); setComputers(false); setInbox(false); setMobileNav(false); history.pushState(null, "", "/workspace/?view=activity"); }}><ActivityIcon size={16}/> My activity</button>
      <button className={`nav-item ${computers ? "active" : ""}`} onClick={() => { setComputers(true); setActivity(false); setInbox(false); setMobileNav(false); history.pushState(null, "", "/workspace/?view=computers"); }}><Monitor size={16}/> Computers</button>
      <button className="nav-item" onClick={() => setUploadsOpen(true)}><Upload size={16}/> Uploads {!!uploads.state.entries.length && <span className="count">{uploads.state.entries.length}</span>}</button>
      <div className="sidebar-section">YOUR CHATS <span>{chats.data?.length ?? 0}</span></div>
      <nav className="chat-list" aria-label="Chats">{chats.data?.map(chat => <button key={chat.id} className={`chat-item ${selected === chat.id && !inbox && !activity && !computers ? "active" : ""}`} onClick={() => select(chat.id)}>
        <MessageSquare size={16}/><span>{chat.title}<small>{chat.archived ? "Archived" : new Date(chat.updatedAt).toLocaleDateString([], { month: "short", day: "numeric" })}</small></span></button>)}
        {chats.data?.length === 0 && <p className="muted">Your chats will appear here.</p>}
        {chats.error && <p className="error">{chats.error.message}</p>}
      </nav>
      <div className="identity"><span className="avatar small">{initials(session.person.displayName)}</span><div><strong>{session.person.displayName}</strong><span>Personal workspace</span></div><button className="icon-button" title="Sign out" aria-label="Sign out" onClick={() => void logout().catch(error => setError(error.message))}><LogOut size={16}/></button></div>
    </aside>
    <main className="main"><div className="mobile-top"><button className="icon-button" aria-label="Open chats" onClick={() => setMobileNav(!mobileNav)}><Menu size={20}/></button><span>Veoveo Workspace</span></div>
      {computers ? <Suspense fallback={<p role="status">Loading Computers…</p>}><Computers session={session} uploads={uploads.state} onUpload={() => setUploadsOpen(true)}/></Suspense> : activity ? <Suspense fallback={<p role="status">Loading activity…</p>}><Activity chats={chats.data}/></Suspense> : inbox ? <div className="inbox"><span className="eyebrow">YOUR WORKSPACE</span><h1>Invitations</h1><p className="muted">Choose the conversations you want to join.</p>
        {invitations.data?.map(({ invitation, chatTitle, inviterName }) => <article className="invitation" key={invitation.id}><div className="avatar"><MessageSquare size={19}/></div><div><h2>{chatTitle}</h2><p>{inviterName} invited you.</p><p className="muted">Joining shares the complete chat history with you. New messages are visible to every member.</p><div className="actions"><button className="primary" disabled={busy} onClick={() => void decide(invitation, "accepted")}>Join chat</button><button disabled={busy} onClick={() => void decide(invitation, "declined")}>Decline</button></div></div></article>)}
        {invitations.data?.length === 0 && <div className="empty-card"><Bell size={25}/><h2>You're all caught up.</h2><p>New chat invitations appear here.</p></div>}
        {invitations.error && <p className="error">{invitations.error.message}</p>}
      </div> : selected ? <Room key={selected} chat={selected} session={session} changed={changed}/> : <div className="welcome"><span className="eyebrow">{session.workContextTitle}</span><div className="spark">✳</div><h1>What shall we work on?</h1><p>Start a conversation. Bring people and agents together.<br/>Keep the work, and the context, in one place.</p><button className="primary" disabled={!session.canContribute} onClick={() => setNewChat(true)}><Plus size={17}/> Start a chat</button><div className="welcome-note"><Users size={16}/> You choose who joins each conversation.</div></div>}
      {error && !newChat && <p className="global-error error" role="alert">{error}</p>}
    </main>
    {newChat && <dialog className="modal-backdrop" ref={node => { if (node && !node.open) node.showModal(); }} onCancel={() => setNewChat(false)}><form className="modal" role="dialog" aria-modal="true" aria-labelledby="new-chat-title" onSubmit={event => { event.preventDefault(); void create(); }}><div className="details-heading"><h2 id="new-chat-title">Start a chat</h2><button type="button" className="icon-button" aria-label="Close" onClick={() => setNewChat(false)}><X size={18}/></button></div><p>Give this conversation a name. You'll be its owner and can invite people from your workspace.</p><label>Chat name<input autoFocus value={title} maxLength={200} readOnly={busy || !!createId.current} onChange={event => setTitle(event.target.value)} placeholder="e.g. Planning our next release"/></label>{error && <p className="error" role="alert">{error}</p>}<button type="submit" className="primary" disabled={busy || !title.trim()}>{busy ? "Creating…" : createId.current ? "Retry creation" : "Create chat"}<ArrowRight size={16}/></button></form></dialog>}
    {uploadsOpen && <Suspense fallback={null}><Uploads queue={uploads.queue} state={uploads.state} close={() => setUploadsOpen(false)}/></Suspense>}
  </div>;
}

function Room({ chat, session, changed }: { chat: string; session: WorkspaceBootstrap; changed: () => void }) {
  const room = useConversation(chat, changed);
  const [details, setDetails] = useState(false);
  const [activity, setActivity] = useState(() => new URLSearchParams(location.search).get("panel") === "activity");
  if (!room.snapshot) return <div className="welcome">{room.error ? <><p className="error" role="alert">{room.error}</p><button onClick={() => void room.refresh()}>Try again</button></> : <p role="status">Loading conversation…</p>}</div>;
  return <><header className="chat-header"><div><h1>{room.snapshot.chat.title}</h1><p><span className={`status-dot ${room.connected ? "online" : ""}`}/>{room.connected ? "Connected" : "Reconnecting"}<span>·</span>{room.snapshot.members.filter(member => member.active).length} people{room.snapshot.activity.agents.some(agent => agent.active) && ` · ${room.snapshot.activity.agents.filter(agent => agent.active).length} agents`}{room.snapshot.chat.archived && " · Archived"}</p></div><div className="actions"><button className={activity ? "active" : ""} onClick={() => { setActivity(!activity); setDetails(false); }}><ActivityIcon size={16}/> Activity</button><button className={details ? "active" : ""} onClick={() => { setDetails(!details); setActivity(false); }}><Users size={16}/> Participants</button></div></header>
    <div className="room-content"><Suspense fallback={<p role="status">Opening conversation…</p>}><Conversation snapshot={room.snapshot} personId={session.person.id} canContribute={session.canContribute} onChanged={room.refresh} onOlder={() => void room.older()} hasOlder={room.hasOlder} loadingOlder={room.loadingOlder}/>
      {activity && <Activity chat={chat} agents={room.snapshot.activity} close={() => setActivity(false)}/>}
      {details && <Participants snapshot={room.snapshot} personId={session.person.id} onChanged={room.refresh} close={() => setDetails(false)}/>}</Suspense>
    </div>{room.error && <p className="global-error error" role="alert">{room.error}</p>}</>;
}
