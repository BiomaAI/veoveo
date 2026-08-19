import { useCallback, useEffect, useState } from "react";
import { Bot, Check, RefreshCw, Send, X } from "lucide-react";
import {
  decideAgentInputRequest,
  loadAgentConversation,
  loadAgentInputRequests,
  sendAgentMessage,
} from "../api";
import { agentDisplayState, uuidV7 } from "../agentControl";
import { IdentityText } from "../components/IdentityText";
import { EmptyState, SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import type {
  AgentConversationEntry,
  AgentInputRequest,
  AgentSummary,
  InstallationSnapshot,
} from "../types";

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

function AgentCard({
  agent,
  directory,
}: {
  agent: AgentSummary;
  directory: InstallationSnapshot;
}) {
  const [message, setMessage] = useState("");
  const [messageRequestId, setMessageRequestId] = useState<string>();
  const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string>();
  const [error, setError] = useState<string>();
  const [conversation, setConversation] = useState<AgentConversationEntry[]>([]);
  const [loadingConversation, setLoadingConversation] = useState(true);
  const [inputRequests, setInputRequests] = useState<AgentInputRequest[]>([]);
  const [loadingInputRequests, setLoadingInputRequests] = useState(true);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [decisionRequests, setDecisionRequests] = useState<Record<string, {
    requestId: string;
    fingerprint: string;
  }>>({});
  const [decidingId, setDecidingId] = useState<string>();
  const displayState = useAgentDisplayState(agent);

  const refreshInputRequests = useCallback(async (signal?: AbortSignal) => {
    setLoadingInputRequests(true);
    try {
      setInputRequests(await loadAgentInputRequests(agent.id, signal));
    } catch (cause) {
      if (!signal?.aborted) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (!signal?.aborted) setLoadingInputRequests(false);
    }
  }, [agent.id]);

  const refreshConversation = useCallback(async (signal?: AbortSignal) => {
    setLoadingConversation(true);
    try {
      const value = await loadAgentConversation(agent.id, signal);
      if (!signal?.aborted) setConversation(value.entries);
    } catch (cause) {
      if (!signal?.aborted) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (!signal?.aborted) setLoadingConversation(false);
    }
  }, [agent.id]);

  useEffect(() => {
    const controller = new AbortController();
    loadAgentInputRequests(agent.id, controller.signal).then(
      (values) => {
        if (!controller.signal.aborted) setInputRequests(values);
      },
      (cause: unknown) => {
        if (!controller.signal.aborted) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      },
    ).finally(() => {
      if (!controller.signal.aborted) setLoadingInputRequests(false);
    });
    return () => controller.abort();
  }, [agent.id, agent.lastEpisodeAt, agent.pendingWakes, agent.state]);

  useEffect(() => {
    const controller = new AbortController();
    loadAgentConversation(agent.id, controller.signal).then(
      (value) => {
        if (!controller.signal.aborted) setConversation(value.entries);
      },
      (cause: unknown) => {
        if (!controller.signal.aborted) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      },
    ).finally(() => {
      if (!controller.signal.aborted) setLoadingConversation(false);
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
      await Promise.all([refreshInputRequests(), refreshConversation()]);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSending(false);
    }
  };

  const decide = async (
    input_request: AgentInputRequest,
    action: "accept" | "decline" | "cancel",
  ) => {
    const decision = action === "accept"
      ? { action, content: { response: answers[input_request.inputRequestId]?.trim() ?? "" } } as const
      : { action } as const;
    const fingerprint = JSON.stringify(decision);
    const existing = decisionRequests[input_request.inputRequestId];
    const requestId = existing?.fingerprint === fingerprint ? existing.requestId : uuidV7();
    setDecisionRequests((current) => ({
      ...current,
      [input_request.inputRequestId]: { requestId, fingerprint },
    }));
    setError(undefined);
    setDecidingId(input_request.inputRequestId);
    try {
      await decideAgentInputRequest(
        agent.id,
        input_request.inputRequestId,
        requestId,
        decision,
      );
      setDecisionRequests((current) => {
        const next = { ...current };
        delete next[input_request.inputRequestId];
        return next;
      });
      setNotice(`Input request ${{ accept: "answered", decline: "declined", cancel: "cancelled" }[action]} and the agent was woken`);
      await refreshInputRequests();
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
      <div className="agent-conversation">
        <div className="agent-conversation-head">
          <strong>Conversation</strong>
          <button
            className="button button-secondary"
            disabled={loadingConversation}
            onClick={() => void refreshConversation()}
            aria-label={`Refresh ${agent.name} conversation`}
          >
            <RefreshCw size={13} className={loadingConversation ? "spin" : ""} />
          </button>
        </div>
        {loadingConversation && conversation.length === 0 ? (
          <span className="subdued">Loading conversation…</span>
        ) : conversation.length === 0 ? (
          <span className="subdued">No conversation yet.</span>
        ) : (
          <div className="agent-conversation-log">
            {conversation.map((entry) => (
              <article className={`agent-conversation-entry ${entry.role}`} key={entry.entryId}>
                <div>
                  <strong>
                    {entry.role === "agent" ? agent.name : (
                      <IdentityText identity={entry.actorId} directory={directory} />
                    )}
                  </strong>
                  <StatusPill value={entry.state} />
                </div>
                {entry.content && <p>{entry.content}</p>}
                <span className="subdued">{formatDate(entry.occurredAt)}</span>
              </article>
            ))}
          </div>
        )}
      </div>
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
      <div className="agent-input-requests">
        <div className="agent-input-requests-head">
          <strong>Waiting for you</strong>
          <button
            className="button button-secondary"
            disabled={loadingInputRequests}
            onClick={() => void refreshInputRequests()}
            aria-label={`Refresh ${agent.name} input requests`}
          >
            <RefreshCw size={13} className={loadingInputRequests ? "spin" : ""} />
          </button>
        </div>
        {loadingInputRequests && inputRequests.length === 0 ? (
          <span className="subdued">Loading input requests…</span>
        ) : inputRequests.length === 0 ? (
          <span className="subdued">No pending input requests.</span>
        ) : inputRequests.map((input_request) => (
          <div className="agent-input-request" key={input_request.inputRequestId}>
            <strong>{input_request.message}</strong>
            <span className="subdued">Requested {formatDate(input_request.requestedAt)}</span>
            <textarea
              value={answers[input_request.inputRequestId] ?? ""}
              onChange={(event) => setAnswers((current) => ({
                ...current,
                [input_request.inputRequestId]: event.target.value,
              }))}
              rows={2}
              placeholder="Response"
            />
            <div className="agent-decision-actions">
              <button className="button button-primary" disabled={decidingId === input_request.inputRequestId} onClick={() => void decide(input_request, "accept")}><Check size={13} /> Answer</button>
              <button className="button button-secondary" disabled={decidingId === input_request.inputRequestId} onClick={() => void decide(input_request, "decline")}><X size={13} /> Decline</button>
              <button className="button button-secondary" disabled={decidingId === input_request.inputRequestId} onClick={() => void decide(input_request, "cancel")}>Cancel request</button>
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
          {snapshot.agents.map((agent) => (
            <AgentCard agent={agent} directory={snapshot} key={agent.id} />
          ))}
        </div>
      )}
    </section>
  );
}
