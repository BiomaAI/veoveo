import { useRef, useState } from "react";
import { api } from "./api.ts";
import { uuidV7 } from "../../console/web/src/agentControl.ts";
import type { AgentRevisionPreview, ChatAgent, UpdateChatAgent } from "./generated/workspace.ts";

export function AgentRevisionUpdate({ chat, agent, latest, changed }: { chat: string; agent: ChatAgent; latest?: string; changed: () => Promise<void> }) {
  const [preview, setPreview] = useState<AgentRevisionPreview>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const pending = useRef<UpdateChatAgent | undefined>(undefined);
  async function review() {
    setBusy(true); setError(undefined);
    try { setPreview(await api.agentRevision(chat, agent.id)); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  async function adopt() {
    if (!preview) return;
    setBusy(true); setError(undefined);
    try {
      pending.current ??= { requestId: uuidV7(), expectedRevision: preview.current.revision, revision: preview.target.revision };
      await api.updateAgent(chat, agent.id, pending.current);
      pending.current = undefined; setPreview(undefined); await changed();
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  const current = preview?.current;
  const target = preview?.target;
  return <div className="agent-revision">
    {latest && latest !== agent.revision && !preview && <button disabled={busy} onClick={() => void review()}>{busy ? "Checking update…" : `Review update for ${agent.name}`}</button>}
    {error && <p className="error" role="alert">{error}</p>}
    {current && target && <div className="agent-disclosure"><h4>Update {agent.name}</h4><p>Running responses keep their original version. Future requests use the version you choose here.</p>
      <dl><dt>Model</dt><dd>{current.model.id} → {target.model.id}{current.model.revision !== target.model.revision ? " (connection changed)" : ""}</dd>
        <dt>Instructions</dt><dd>{current.instructionsDigest === target.instructionsDigest ? "Unchanged" : "Changed"}{target.instructions && <details><summary>Read new instructions</summary><pre style={{ whiteSpace:"pre-wrap" }}>{target.instructions}</pre></details>}</dd>
        <dt>Capabilities added</dt><dd>{target.tools.filter(t => !current.tools.includes(t)).join(", ") || "None"}</dd>
        <dt>Capabilities removed</dt><dd>{current.tools.filter(t => !target.tools.includes(t)).join(", ") || "None"}</dd>
        <dt>Output tokens</dt><dd>{current.budgets.maxOutputTokens} → {target.budgets.maxOutputTokens}</dd>
        <dt>Model calls</dt><dd>{current.budgets.maxCompletionCalls} → {target.budgets.maxCompletionCalls}</dd>
        <dt>Tool calls</dt><dd>{current.budgets.maxToolCalls} → {target.budgets.maxToolCalls}</dd>
        <dt>Time limit</dt><dd>{current.budgets.deadlineSeconds}s → {target.budgets.deadlineSeconds}s</dd>
      </dl><p className="muted">Published {new Date(target.publishedAt).toLocaleString()} by {target.publishedByName}.</p>
      <div className="actions"><button disabled={busy} onClick={() => void adopt()}>{busy ? "Updating…" : pending.current ? "Retry update" : "Use this version"}</button><button disabled={busy} onClick={() => { pending.current = undefined; setPreview(undefined); }}>Close</button></div>
    </div>}
  </div>;
}
