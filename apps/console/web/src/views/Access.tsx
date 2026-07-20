import { useState, type ReactNode } from "react";
import { Check, ShieldCheck, X } from "lucide-react";
import { SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import {
  useArtifactAccessRequests,
  useCancelArtifactAccessRequest,
  useDecideArtifactAccessRequest,
} from "../queries";
import type { ArtifactAccessRequest, InstallationSnapshot, WorkContextMembership } from "../types";

const membershipRank: Record<WorkContextMembership, number> = {
  viewer: 0,
  contributor: 1,
  custodian: 2,
  owner: 3,
};

export function AccessView({ snapshot }: { snapshot: InstallationSnapshot }) {
  const canReview = membershipRank[snapshot.session.membership] >= membershipRank.custodian;
  const mine = useArtifactAccessRequests("mine");
  const reviewable = useArtifactAccessRequests("reviewable", undefined, canReview);
  const decide = useDecideArtifactAccessRequest();
  const cancel = useCancelArtifactAccessRequest();
  const [notes, setNotes] = useState<Record<string, string>>({});
  const [actionError, setActionError] = useState<string>();

  const artifactName = (request: ArtifactAccessRequest) =>
    snapshot.artifacts.find((artifact) => artifact.id === request.artifactId)?.filename ??
    request.artifactId;

  const review = async (requestId: string, decision: "approve" | "deny") => {
    setActionError(undefined);
    try {
      await decide.mutateAsync({
        requestId,
        decision,
        note: notes[requestId]?.trim() || undefined,
      });
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Access decision failed");
    }
  };

  const cancelRequest = async (requestId: string) => {
    setActionError(undefined);
    try {
      await cancel.mutateAsync(requestId);
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Access request cancellation failed");
    }
  };

  return (
    <div className="access-layout">
      <section className="panel access-wide context-banner">
        <ShieldCheck size={22} />
        <div>
          <strong>{snapshot.session.workContextTitle}</strong>
          <span className="mono">{snapshot.session.workContext}</span>
        </div>
        <dl>
          <div><dt>Membership</dt><dd><StatusPill value={snapshot.session.membership} /></dd></div>
          <div><dt>Invocation</dt><dd>{snapshot.session.invocationMode}</dd></div>
          <div><dt>Actor</dt><dd className="mono">{snapshot.session.actorId}</dd></div>
        </dl>
      </section>

      {actionError && <div className="action-error access-wide">{actionError}</div>}

      <AccessRequestPanel
        title="My access requests"
        requests={mine.data?.requests}
        loading={mine.isLoading}
        error={mine.error}
        artifactName={artifactName}
        actions={(request) =>
          request.state === "pending" ? (
            <button
              className="button button-secondary"
              disabled={cancel.isPending}
              onClick={() => void cancelRequest(request.id)}
            >
              Cancel
            </button>
          ) : null
        }
      />

      {canReview ? (
        <AccessRequestPanel
          title="Review queue"
          requests={reviewable.data?.requests}
          loading={reviewable.isLoading}
          error={reviewable.error}
          artifactName={artifactName}
          actions={(request) =>
            request.state === "pending" ? (
              <div className="review-actions">
                <input
                  value={notes[request.id] ?? ""}
                  onChange={(event) =>
                    setNotes((current) => ({ ...current, [request.id]: event.target.value }))
                  }
                  placeholder="Decision note"
                  maxLength={4096}
                  aria-label="Decision note"
                />
                <button
                  className="icon-button"
                  title="Approve access"
                  disabled={decide.isPending}
                  onClick={() => void review(request.id, "approve")}
                >
                  <Check size={15} />
                </button>
                <button
                  className="icon-button icon-danger"
                  title="Deny access"
                  disabled={decide.isPending}
                  onClick={() => void review(request.id, "deny")}
                >
                  <X size={15} />
                </button>
              </div>
            ) : null
          }
        />
      ) : (
        <section className="panel">
          <SectionHeader title="Review queue" count={0} />
          <p className="panel-intro">
            Custodian or owner membership in the active Work Context is required to review access
            requests.
          </p>
        </section>
      )}

      <section className="panel access-wide">
        <SectionHeader title="Active policy sets" count={snapshot.policies.length} />
        <p className="panel-intro">
          Gateway policy is activated as a versioned control-plane revision. Work Context
          membership and artifact grants remain visible as separate authority sources.
        </p>
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Policy</th>
                <th>State</th>
                <th>Revision</th>
                <th>Rules</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {snapshot.policies.map((policy) => (
                <tr key={policy.id}>
                  <td>
                    <strong>{policy.name}</strong>
                    <span className="mono subdued">{policy.id}</span>
                  </td>
                  <td><StatusPill value={policy.state} /></td>
                  <td>r{policy.revision}</td>
                  <td>{policy.rules}</td>
                  <td>{formatDate(policy.updatedAt)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function AccessRequestPanel({
  title,
  requests,
  loading,
  error,
  artifactName,
  actions,
}: {
  title: string;
  requests: ArtifactAccessRequest[] | undefined;
  loading: boolean;
  error: Error | null;
  artifactName: (request: ArtifactAccessRequest) => string;
  actions: (request: ArtifactAccessRequest) => ReactNode;
}) {
  return (
    <section className="panel">
      <SectionHeader title={title} count={requests?.length ?? 0} />
      {loading && <p className="panel-intro">Loading governed requests…</p>}
      {error && <div className="action-error">{error.message}</div>}
      {!loading && !requests?.length && <div className="empty-panel">No access requests</div>}
      {!!requests?.length && (
        <div className="access-request-list">
          {requests.map((request) => (
            <article key={request.id}>
              <div className="access-request-head">
                <div>
                  <strong>{artifactName(request)}</strong>
                  <span className="mono">{request.artifactId}</span>
                </div>
                <StatusPill value={request.state} />
              </div>
              <p>{request.justification}</p>
              <dl>
                <div><dt>Requester</dt><dd>{request.requester}</dd></div>
                <div><dt>Level</dt><dd>{request.requestedLevel}</dd></div>
                <div><dt>Context</dt><dd>{request.workContext}</dd></div>
                <div><dt>Updated</dt><dd>{formatDate(request.updatedAt)}</dd></div>
              </dl>
              {request.decisionNote && <p className="decision-note">{request.decisionNote}</p>}
              {actions(request)}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
