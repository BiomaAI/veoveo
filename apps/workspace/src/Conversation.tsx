import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import { AssistantRuntimeProvider, MessagePrimitive, ThreadPrimitive, useAuiState,
  useExternalMessageConverter, useExternalStoreRuntime } from "@assistant-ui/react";
import { ArrowUp, Bot, CornerDownLeft, Reply, Square, X } from "lucide-react";
import { api, ApiError, type ConversationSnapshot } from "./api.ts";
import { addressedAgents, responseAgents } from "./participation.ts";
import { initials } from "./identity.ts";
import { present, quote, toThreadMessage, type PresentedMessage } from "./conversation.ts";
import type { ReplyContext, SendMessage } from "./generated/workspace.ts";

const RunActions = createContext<(id: string) => void>(() => {});
const ReplyActions = createContext<{ messages: PresentedMessage[]; disabled: boolean; select: (message: PresentedMessage) => void }>({ messages: [], disabled: true, select: () => {} });

function ReplyQuote({ context }: { context: ReplyContext }) {
  return <blockquote className="reply-quote"><strong>Reply to {context.authorName}</strong><span>{context.text || "Response without text"}</span></blockquote>;
}

function ChatMessage() {
  const cancel = useContext(RunActions);
  const replies = useContext(ReplyActions);
  const message = useAuiState(state => state.message);
  const meta = message.metadata.custom;
  const name = typeof meta.authorName === "string" ? meta.authorName : "Participant";
  const agent = meta.kind === "agent";
  const source = replies.messages.find(item => item.id === message.id);
  const context = source && quote(source, replies.messages);
  const responding = meta.runState === "queued" || meta.runState === "running";
  return <MessagePrimitive.Root data-message-id={message.id} className={`message ${meta.mine === true ? "mine" : ""}`}>
    <div className={`avatar ${agent ? "agent-avatar" : ""}`} aria-hidden="true">{agent ? <Bot size={17}/> : initials(name)}</div>
    <div className="message-body"><div className="message-author"><strong>{name}</strong>{agent && <span className="badge">Agent</span>}
      <time dateTime={message.createdAt.toISOString()}>{message.createdAt.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</time>
      {source && <button className="reply-button" aria-label={`Reply to ${name}`} title={responding ? "Wait for this response to finish" : `Reply to ${name}`} disabled={replies.disabled || responding}
        onClick={() => replies.select(source)}><Reply size={13}/> Reply</button>}</div>
      {context && <ReplyQuote context={context}/>}
      <div className="message-text"><MessagePrimitive.Parts /></div>
      {agent && <div className="run-status" role="status">
        <span>{meta.runState === "queued" ? "Starting…" : meta.runState === "running" ? "Responding…" : meta.runState === "interrupted" ? "Response interrupted. Send a new request to try again." : meta.runState === "cancelled" ? "Response stopped" : meta.runState === "failed" ? meta.runFailure === "capacity" ? "Your message was sent. This agent could not start because response capacity is full." : "The response could not be completed." : ""}</span>
        {(meta.runState === "running" || meta.runState === "queued") && meta.canCancel === true && typeof meta.runId === "string" && <button className="stop-response" aria-label={`Stop ${name}'s response`} onClick={() => cancel(meta.runId as string)}><Square size={11}/> Stop response</button>}
      </div>}
    </div>
  </MessagePrimitive.Root>;
}

export function Conversation({ snapshot, personId, canContribute, onChanged, onOlder, hasOlder, loadingOlder }: {
  snapshot: ConversationSnapshot; personId: string; canContribute: boolean; onChanged: () => Promise<void>;
  onOlder: () => void; hasOlder: boolean; loadingOlder: boolean;
}) {
  const source = useMemo(() => present(snapshot, personId, snapshot.activity), [snapshot, personId]);
  const converted = useExternalMessageConverter({ callback: toThreadMessage, messages: source, isRunning: false, joinStrategy: "none" });
  // The room is always writable while individual runs execute. Our composer owns
  // stable send identities; assistant-ui owns presentation and scroll behavior.
  const runtime = useExternalStoreRuntime({ messages: converted, isRunning: false, onNew: async () => {} });
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string>();
  const [selected, setSelected] = useState<string[]>([]);
  const [reply, setReply] = useState<PresentedMessage>();
  const composer = useRef<HTMLTextAreaElement>(null);
  const activeAgents = snapshot.activity.agents.filter(agent => agent.active);
  const attempt = useRef<SendMessage | undefined>(undefined);
  let addressed: string[] = [], responders: string[] = [], selectionError: string | undefined;
  try { addressed = addressedAgents(draft, selected, activeAgents); responders = responseAgents(snapshot.chat.participation, addressed); }
  catch (error) { selectionError = error instanceof Error ? error.message : "Update your agent selection."; }
  const cancel = useCallback(async (id: string) => {
    try { await api.cancelRun(snapshot.chat.id, id); await onChanged(); }
    catch (error) { setError(error instanceof Error ? error.message : "Could not stop this response."); }
  }, [snapshot.chat.id, onChanged]);
  const send = useCallback(async () => {
    if (pending || !draft.trim() || snapshot.chat.archived || !canContribute) return;
    const text = draft.trim();
    if (new TextEncoder().encode(text).length > 32768) { setError("Keep the message under 32 KB."); return; }
    if (attempt.current && attempt.current.text !== text) {
      setError("Retry the original message first to confirm whether it was sent."); return;
    }
    if (selectionError && !attempt.current) { setError(selectionError); return; }
    const request = attempt.current ?? { id: crypto.randomUUID(), text, replyTo: reply?.target ?? null, addressedAgents: addressed };
    attempt.current = request;
    setPending(true); setError(undefined);
    try {
      await api.send(snapshot.chat.id, request);
      attempt.current = undefined; setDraft(""); setReply(undefined);
      await onChanged();
    } catch (error) {
      if (error instanceof ApiError && [400, 401, 403, 404, 409, 422].includes(error.status)) attempt.current = undefined;
      setError(error instanceof Error ? error.message : "The message could not be confirmed.");
    }
    finally { setPending(false); }
  }, [pending, draft, reply, snapshot.chat, canContribute, onChanged, addressed, selectionError]);
  const selectReply = (message: PresentedMessage) => {
    if (pending || attempt.current || snapshot.chat.archived || !canContribute) return;
    setReply(message);
    if (message.kind === "agent" && activeAgents.some(agent => agent.id === message.authorId)) setSelected([message.authorId]);
    composer.current?.focus();
  };
  return <RunActions.Provider value={id => void cancel(id)}><ReplyActions.Provider value={{ messages: source, disabled: pending || !!attempt.current || snapshot.chat.archived || !canContribute, select: selectReply }}><AssistantRuntimeProvider runtime={runtime}>
    <ThreadPrimitive.Root className="conversation">
      <ThreadPrimitive.Viewport className="timeline">
        <div className="timeline-inner">
          {hasOlder && <button className="older" onClick={onOlder} disabled={loadingOlder}>{loadingOlder ? "Loading…" : "Load earlier messages"}</button>}
          {snapshot.messages.length === 0 && <div className="chat-empty"><div className="spark">✳</div><h2>A place to work together.</h2><p>Write a message, invite your people, and bring your work into the conversation.</p></div>}
          <ThreadPrimitive.Messages components={{ Message: ChatMessage }} />
        </div>
      </ThreadPrimitive.Viewport>
      <div className="composer-area">
        {activeAgents.length > 0 && <fieldset className="agent-targets" disabled={pending || !!attempt.current || snapshot.chat.archived || !canContribute}>
          <legend>Ask an agent</legend>{activeAgents.map(agent => <label key={agent.id} className={selected.includes(agent.id) ? "selected" : ""}>
            <input type="checkbox" checked={selected.includes(agent.id)} onChange={event => setSelected(current => event.target.checked ? [...current, agent.id] : current.filter(id => id !== agent.id))}/><Bot size={13}/>{agent.name}
          </label>)}
        </fieldset>}
        {selected.some(id => !activeAgents.some(agent => agent.id === id)) && <p className="error" role="status">
          An addressed agent left this chat. <button disabled={pending || !!attempt.current}
            onClick={() => setSelected(current => current.filter(id => activeAgents.some(agent => agent.id === id)))}>Clear unavailable agents</button>
        </p>}
        {activeAgents.length > 0 && <p className="composer-note" role="status">{selectionError ?? (responders.length
          ? `Will respond: ${responders.map(id => activeAgents.find(agent => agent.id === id)?.name ?? "Agent").join(", ")}.`
          : "No agent response requested.")} Begin with @Name or select an agent above.</p>}
        {error && <p role="alert" className="error">{error} {attempt.current && "Your text is kept here; retry uses the same message ID."}</p>}
        {reply && <div className="composer-reply" role="region" aria-label="Reply context"><ReplyQuote context={{ authorName: reply.authorName, text: [...reply.text].slice(0, 500).join("") }}/>
          <button className="icon-button" aria-label="Remove reply" disabled={pending || !!attempt.current} onClick={() => { setReply(undefined); composer.current?.focus(); }}><X size={16}/></button></div>}
        <form className="composer" onSubmit={event => { event.preventDefault(); void send(); }}>
          <textarea ref={composer} aria-label="Message" placeholder={snapshot.chat.archived ? "This chat is archived" : "Write to everyone in this chat…"}
            value={draft} disabled={!canContribute || snapshot.chat.archived} readOnly={pending || !!attempt.current}
            rows={2} onChange={event => setDraft(event.target.value)} onKeyDown={event => {
              if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); void send(); }
            }} />
          <div className="composer-footer"><span><CornerDownLeft size={12}/> Enter to send · Shift + Enter for a new line</span>
            <button className="send" type="submit" disabled={pending || !draft.trim() || snapshot.chat.archived || !canContribute || (!!selectionError && !attempt.current)} aria-label={attempt.current ? "Retry message" : "Send message"}><ArrowUp size={19}/></button>
          </div>
        </form>
        <p className="composer-note">Everyone in this chat can read its shared history.</p>
      </div>
    </ThreadPrimitive.Root>
  </AssistantRuntimeProvider></ReplyActions.Provider></RunActions.Provider>;
}
