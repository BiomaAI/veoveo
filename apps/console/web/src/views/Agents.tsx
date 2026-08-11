import { useCallback, useEffect, useState } from "react";
import { Bot, Check, RefreshCw, Send, X } from "lucide-react";
import {
  decideAgentElicitation,
  loadAgentElicitations,
  sendAgentMessage,
} from "../api";
import { agentDisplayState, uuidV7 } from "../agentControl";
import { EmptyState, SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import type { AgentElicitation, AgentSummary, InstallationSnapshot } from "../types";

function useAgentDisplayState(agent: AgentSummary) {
  const [, refreshAtExpiry] = useState(0);
  useEffect(() => {
    const expiresAt = Date.parse(agent.runnerLeaseExpiresAt ?? "");
    if (!Number.isFinite(expiresAt) || expiresAt <= Date.now()) return;
    let timer: number | undefined;
    const schedule = () => {
      const remaining = expiresAt - Date.now();
      if (remaining <= 0) {
        refreshAtExpiry((revision) => revision + 1);
        return;
      }
      timer = window.setTimeout(schedule, Math.min(remaining + 25, 2_147_483_647));
    };
    schedule();
    return () => window.clearTimeout(timer);
  }, [agent.runnerLeaseExpiresAt]);
  return agentDisplayState(agent);
}

function AgentCard({ agent }: { agent: AgentSummary }) {
  const [message, setMessage] = useState("");
  const [messageRequestId, setMessageRequestId] = useState<string>();
  const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string>();
  const [error, setError] = useState<string>();
  const [elicitations, setElicitations] = useState<AgentElicitation[]>([]);
  const [loadingElicitations, setLoadingElicitations] = useState(true);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [decisionRequests, setDecisionRequests] = useState<Record<string, {
    requestId: string;
    fingerprint: string;
  }>>({});
  const [decidingId, setDecidingId] = useState<string>();
  const displayState = useAgentDisplayState(agent);

  const refreshElicitations = useCallback(async (signal?: AbortSignal) => {
    setLoadingElicitations(true);
    try {
      setElicitations(await loadAgentElicitations(agent.id, signal));
    } catch (cause) {
      if (!signal?.aborted) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (!signal?.aborted) setLoadingElicitations(false);
    }
  }, [agent.id]);

  useEffect(() => {
    const controller = new AbortController();
    loadAgentElicitations(agent.id, controller.signal).then(
      (values) => {
        if (!controller.signal.aborted) setElicitations(values);
      },
      (cause: unknown) => {
        if (!controller.signal.aborted) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      },
    ).finally(() => {
      if (!controller.signal.aborted) setLoadingElicitations(false);
    });
    return () => controller.abort();
  }, [agent.id, agent.lastEpisodeAt, agent.pendingWakes, agent.state]);

  const submitMessage = async () => {
    const value = message.trim();
    if (!value || sending) return;
    if (new TextEncoder().encode(value).length > 16 * 1024) {
      setError("Agent messages may contain at most 16 KiB of UTF-8 text.");
      return;
    }
    const requestId = messageRequestId ?? uuidV7();
    setMessageRequestId(requestId);
    setSending(true);
    setError(undefined);
    try {
      const receipt = await sendAgentMessage(agent.id, requestId, value);
      setMessage("");
      setMessageRequestId(undefined);
      setNotice(`Accepted as wake ${receipt.wakeId}`);
      await refreshElicitations();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSending(false);
    }
  };

  const decide = async (
    elicitation: AgentElicitation,
    action: "accept" | "decline" | "cancel",
  ) => {
    const decision = action === "accept"
      ? { action, content: { response: answers[elicitation.elicitationId]?.trim() ?? "" } } as const
      : { action } as const;
    const fingerprint = JSON.stringify(decision);
    const existing = decisionRequests[elicitation.elicitationId];
    const requestId = existing?.fingerprint === fingerprint ? existing.requestId : uuidV7();
    setDecisionRequests((current) => ({
      ...current,
      [elicitation.elicitationId]: { requestId, fingerprint },
    }));
    setError(undefined);
    setDecidingId(elicitation.elicitationId);
    try {
      await decideAgentElicitation(
        agent.id,
        elicitation.elicitationId,
        requestId,
        decision,
      );
      setDecisionRequests((current) => {
        const next = { ...current };
        delete next[elicitation.elicitationId];
        return next;
      });
      setNotice(`Elicitation ${{ accept: "answered", decline: "declined", cancel: "cancelled" }[action]} and the agent was woken`);
      await refreshElicitations();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setDecidingId(undefined);
    }
  };

  return (
    <article className="item-card agent-card">
      <div className="item-card-head">
        <div className="object-icon"><Bot size={18} /></div>
        <StatusPill value={displayState} />
      </div>
      <h3>{agent.name}</h3>
      <span className="mono subdued">{agent.id}</span>
      <dl>
        <div><dt>Profile</dt><dd>{agent.profile}</dd></div>
        <div><dt>Pending wakes</dt><dd>{agent.pendingWakes}</dd></div>
        <div><dt>Last episode</dt><dd>{formatDate(agent.lastEpisodeAt)}</dd></div>
      </dl>
      <p className="agent-detail">{agent.detail}</p>
      <div className="agent-control">
        <label>
          <span>Message this agent</span>
          <textarea
            value={message}
            onChange={(event) => {
              setMessage(event.target.value);
              setMessageRequestId(undefined);
              setNotice(undefined);
            }}
            maxLength={16_384}
            rows={3}
            placeholder="Describe a change or ask the agent to react now…"
          />
        </label>
        <button
          className="button button-primary"
          disabled={sending || !message.trim()}
          onClick={() => void submitMessage()}
        >
          <Send size={14} /> {sending ? "Sending…" : "Send now"}
        </button>
        <p className="control-help">Accepted messages become durable, non-coalesced priority wakes even while an episode is running.</p>
      </div>
      <div className="agent-elicitations">
        <div className="agent-elicitations-head">
          <strong>Waiting for you</strong>
          <button
            className="button button-secondary"
            disabled={loadingElicitations}
            onClick={() => void refreshElicitations()}
            aria-label={`Refresh ${agent.name} elicitations`}
          >
            <RefreshCw size={13} className={loadingElicitations ? "spin" : ""} />
          </button>
        </div>
        {loadingElicitations && elicitations.length === 0 ? (
          <span className="subdued">Loading elicitations…</span>
        ) : elicitations.length === 0 ? (
          <span className="subdued">No parked elicitations.</span>
        ) : elicitations.map((elicitation) => (
          <div className="agent-elicitation" key={elicitation.elicitationId}>
            <strong>{elicitation.message}</strong>
            <span className="subdued">Requested {formatDate(elicitation.requestedAt)}</span>
            <textarea
              value={answers[elicitation.elicitationId] ?? ""}
              onChange={(event) => setAnswers((current) => ({
                ...current,
                [elicitation.elicitationId]: event.target.value,
              }))}
              rows={2}
              placeholder="Response"
            />
            <div className="agent-decision-actions">
              <button className="button button-primary" disabled={decidingId === elicitation.elicitationId} onClick={() => void decide(elicitation, "accept")}><Check size={13} /> Answer</button>
              <button className="button button-secondary" disabled={decidingId === elicitation.elicitationId} onClick={() => void decide(elicitation, "decline")}><X size={13} /> Decline</button>
              <button className="button button-secondary" disabled={decidingId === elicitation.elicitationId} onClick={() => void decide(elicitation, "cancel")}>Cancel request</button>
            </div>
          </div>
        ))}
      </div>
      {notice && <p className="action-success">{notice}</p>}
      {error && <p className="action-error">{error}</p>}
    </article>
  );
}

export function AgentsView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return (
    <section className="panel full-panel">
      <SectionHeader title="Agents" count={snapshot.agents.length} />
      <p className="panel-intro">Agents stay addressable while idle, reasoning, waiting, or processing prior work. A new message does not stop the work already in flight.</p>
      {snapshot.agents.length === 0 ? (
        <EmptyState>No agents are registered in this Work Context.</EmptyState>
      ) : (
        <div className="item-grid agent-grid">
          {snapshot.agents.map((agent) => <AgentCard agent={agent} key={agent.id} />)}
        </div>
      )}
    </section>
  );
}
