import { useCallback, useEffect, useMemo, useState } from "react";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { Activity as ActivityIcon, Check, CircleAlert, Clock3, LockKeyhole, RefreshCw, Square, X } from "lucide-react";
import { api, ApiError } from "./api.ts";
import { TaskInput } from "./TaskInput.tsx";
import { ResourceResult } from "./ResourceResult.tsx";
import type { AgentActivity, Chat, InputAnswer, OperationSummary, OperationView } from "./generated/workspace.ts";

function active(view?: OperationView): boolean {
  return !!view && (view.operation.phase === "dispatching" || view.task?.state === "working" || view.task?.state === "input_required");
}
function interval(view?: OperationView): number | false {
  if (!view) return false;
  if (view.operation.phase === "dispatching") return 1500;
  const wait = Math.max(5000, view.task?.pollIntervalMs ?? 5000);
  return active(view) && wait <= 2_147_483_647 ? wait : false;
}
function label(view: OperationView): string {
  if (view.result?.isError) return "Completed with an error";
  switch (view.task?.state ?? view.operation.phase) {
    case "dispatching": return "Starting operation";
    case "input_required": return "Needs your input";
    case "completed": return "Completed";
    case "failed": return "Failed";
    case "cancelled": return "Cancelled";
    case "unconfirmed": return "Outcome unconfirmed";
    default: return "Working";
  }
}

export function Activity({ chat, close, agents, chats }: { chat?: string; close?: () => void; agents?: AgentActivity; chats?: Chat[] }) {
  const client = useQueryClient();
  const [visible, setVisible] = useState(10);
  const [activeIds, setActiveIds] = useState<Set<string>>(() => new Set());
  const observed = useCallback((id: string, isActive: boolean) => setActiveIds(current => {
    if (current.has(id) === isActive) return current;
    const next = new Set(current); if (isActive) next.add(id); else next.delete(id); return next;
  }), []);
  const query = useInfiniteQuery({ queryKey: ["operations", chat], initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam, signal }) => api.operations(chat, pageParam, signal), getNextPageParam: page => page.next ?? undefined,
    refetchInterval: 10_000, refetchIntervalInBackground: false });
  const items = useMemo(() => query.data?.pages.flatMap(page => page.items) ?? [], [query.data]);
  const ids = items.slice(0, visible).filter(item => activeIds.has(item.id)).slice(0, 32).map(item => item.id).sort().join(",");
  useEffect(() => {
    if (!ids) return;
    const source = new EventSource(`/workspace/api/operations/events?${new URLSearchParams({ ids })}`);
    let timer: ReturnType<typeof setTimeout> | undefined;
    function refresh() {
      if (timer) return;
      timer = setTimeout(() => { timer = undefined; for (const id of ids.split(",")) void client.invalidateQueries({ queryKey: ["operation", id] }); }, 200);
    }
    source.addEventListener("change", refresh);
    source.addEventListener("expired", refresh);
    // Reconnect GETs and bounded Tasks reconciliation never submit work.
    return () => { source.close(); if (timer) clearTimeout(timer); };
  }, [ids, client]);
  return <section className={chat ? "task-panel" : "activity-page"} aria-label={chat ? "Your activity in this chat" : "My activity"}>
    <div className="details-heading"><h2><ActivityIcon size={18}/> {chat ? "Your activity" : "My activity"}</h2>{close && <button className="icon-button" aria-label="Close activity" onClick={close}><X size={18}/></button>}</div>
    <p className="activity-privacy"><LockKeyhole size={13}/> Task details and input requests are private to you.</p>
    {query.isPending && <p role="status">Loading your activity…</p>}
    {query.error && <p className="error" role="alert">{query.error.message}<button onClick={() => void query.refetch()}>Refresh activity</button></p>}
    {query.data && !items.length && <div className="empty-card"><ActivityIcon size={24}/><h3>No activity yet</h3><p>Tasks from your tools, Apps and agents will appear here. You can return to them after leaving a chat.</p></div>}
    <div className="task-list">{items.slice(0, visible).map(operation => {
      const run = agents?.runs.find(run => run.id === operation.runId);
      const agent = agents?.agents.find(agent => agent.id === run?.agent);
      return <TaskCard key={operation.id} operation={operation} observed={observed} agent={agent?.name} chatTitle={chats?.find(chat => chat.id === operation.chatId)?.title} showOrigin={!chat}/>;
    })}</div>
    {(visible < items.length || query.hasNextPage) && <button disabled={query.isFetchingNextPage} onClick={() => {
      setVisible(count => count + 10); if (visible >= items.length) void query.fetchNextPage();
    }}>{query.isFetchingNextPage ? "Loading…" : "Load earlier activity"}</button>}
  </section>;
}

function TaskCard({ operation, observed, agent, chatTitle, showOrigin }: { operation: OperationSummary; observed: (id: string, active: boolean) => void; agent?: string; chatTitle?: string; showOrigin: boolean }) {
  const query = useQuery({ queryKey: ["operation", operation.id], queryFn: ({ signal }) => api.operation(operation.id, signal),
    refetchInterval: query => query.state.error ? false : interval(query.state.data), refetchIntervalInBackground: false, retry: false });
  const [busy, setBusy] = useState(false);
  const [cancelAsked, setCancelAsked] = useState(false);
  const [error, setError] = useState<string>();
  const view = query.data;
  const watch = !query.error && !!view?.task && active(view);
  useEffect(() => { observed(operation.id, watch); return () => observed(operation.id, false); }, [operation.id, watch, observed]);
  async function cancel() {
    if (busy) return; setBusy(true); setError(undefined);
    try { await api.cancelOperation(operation.id); setCancelAsked(true); }
    catch { setError("Cancellation could not be confirmed. Refresh the task to check its current state."); }
    finally { await query.refetch(); setBusy(false); }
  }
  async function answer(answers: InputAnswer[]) {
    if (busy || !view) return; setBusy(true); setError(undefined);
    try { await api.answerOperation(operation.id, { revision: view.operation.revision, answers }); }
    catch (error) { setError(error instanceof ApiError && error.status === 409 ? "This input request changed. Review the current request below." : "Your answer could not be confirmed. The current request has been refreshed."); }
    finally { await query.refetch(); setBusy(false); }
  }
  const state = view?.task?.state ?? view?.operation.phase;
  const completed = ["completed", "failed", "cancelled"].includes(state ?? "");
  return <article className="task-card" aria-label={`Activity: ${operation.tool}`}>
    <div className="task-heading"><span className={`task-icon ${completed ? "settled" : ""}`}>{state === "completed" && !view?.result?.isError ? <Check size={16}/> : state === "input_required" || state === "failed" || state === "unconfirmed" || view?.result?.isError ? <CircleAlert size={16}/> : <Clock3 size={16}/>}</span>
      <div><h3>{operation.tool.replaceAll("__", " · ").replaceAll("_", " ")}</h3><time dateTime={operation.createdAt}>{new Date(operation.createdAt).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}</time></div>
      <button className="icon-button" aria-label="Refresh task" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw size={14}/></button></div>
    <p className="task-origin muted">{operation.runId ? `${agent ?? "Agent"} · Requested for you` : "Requested by you"}{showOrigin && <> · <a href={`/workspace/?chat=${operation.chatId}&panel=activity`}>{chatTitle ?? "Open originating chat"}</a></>}</p>
    {query.isPending && <p role="status">Loading current status…</p>}
    {query.error ? <p className="error" role="alert">{query.error instanceof ApiError && [403, 404].includes(query.error.status) ? "This activity is no longer available with your access, or its retention period has ended." : "Current status is unavailable. The operation has not been restarted."}</p> : view && <>
      <div className="task-state" role="status">{label(view)}</div>
      {view.task?.message && <p>{view.task.message}</p>}
      {active(view) && state !== "input_required" && <div className="task-progress" role="progressbar" aria-label="Task in progress"><span/></div>}
      {state === "unconfirmed" && <p>The connection ended before the outcome was recorded. This operation will not be submitted again automatically.</p>}
      {view.inputs.length > 0 && <TaskInput independent={!!view.task} inputs={view.inputs} busy={busy} onAnswer={answers => void answer(answers)} onError={setError}/>}
      {view.operation.phase === "input_required" && view.inputs.length === 0 && <button className="primary" disabled={busy} onClick={() => void answer([])}>Continue operation</button>}
      {view.result && <div className="task-result">{view.result.text.map((text, index) => <p key={index}>{text}</p>)}
        {view.result.resources.map(resource => <ResourceResult key={resource.uri} resource={resource}/>)}
        {view.result.structured != null && <details><summary>Result details</summary><pre>{JSON.stringify(view.result.structured, null, 2)}</pre></details>}
      </div>}
      {view.task && !completed && <div className="task-actions"><button disabled={busy || cancelAsked} onClick={() => void cancel()}><Square size={12}/> {cancelAsked ? "Cancellation requested" : "Cancel task"}</button>{cancelAsked && <small>Waiting for the server to confirm the outcome.</small>}</div>}
    </>}
    {error && <p className="error" role="alert">{error}</p>}
  </article>;
}
