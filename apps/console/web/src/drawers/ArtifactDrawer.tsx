import { useState, type FormEvent } from "react";
import { Check, Copy, Download, FileStack, Fingerprint, Link2, ShieldCheck, Trash2, UserRound } from "lucide-react";
import { DrawerShell } from "./DrawerShell";
import { StatusPill } from "../components/primitives";
import { useConfirm } from "../components/confirm";
import { artifactDownloadUrl } from "../api";
import { formatBytes, formatDate } from "../format";
import {
  useCreateShareLink,
  useGrantArtifact,
  useRevokeArtifactGrant,
  useRevokeShareLink,
  useSetArtifactReleaseState,
} from "../queries";
import type { ArtifactSummary, ReleaseState } from "../types";
import { ArtifactPreview } from "../components/ArtifactPreview";

export function ArtifactDrawer({
  artifact,
  principalId,
  onClose,
  onOpenRecording,
}: {
  artifact: ArtifactSummary;
  principalId: string;
  onClose: () => void;
  onOpenRecording: (recordingId: string) => void;
}) {
  const confirm = useConfirm();
  const setReleaseState = useSetArtifactReleaseState();
  const grantAccess = useGrantArtifact();
  const revokeGrant = useRevokeArtifactGrant();
  const createLink = useCreateShareLink();
  const revokeLink = useRevokeShareLink();

  const [copied, setCopied] = useState(false);
  const [linkCopied, setLinkCopied] = useState(false);
  const [newLink, setNewLink] = useState<string>();
  const [actionError, setActionError] = useState<string>();
  const [subjectKind, setSubjectKind] = useState<"principal" | "group">("principal");
  const [subjectId, setSubjectId] = useState("");
  const [grantLevel, setGrantLevel] = useState<"read" | "write" | "admin">("read");
  const [linkDays, setLinkDays] = useState(7);
  const [maxDownloads, setMaxDownloads] = useState("");

  const pending =
    setReleaseState.isPending || grantAccess.isPending || revokeGrant.isPending || createLink.isPending || revokeLink.isPending;
  const recordingRelationTitle =
    artifact.recording?.kind === "recording_manifest"
      ? "Recording manifest"
      : artifact.recording?.kind === "recording_segment"
        ? `Recording segment ${artifact.recording.ordinal ?? ""}`.trim()
        : "Derived from recording";
  const canAdminister = artifact.effectiveAccess.admin;

  const copyId = async () => {
    await navigator.clipboard.writeText(`artifact://${artifact.id}`);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  const run = async (operation: () => Promise<unknown>, fallback: string) => {
    setActionError(undefined);
    try {
      await operation();
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : fallback);
    }
  };

  const changeReleaseState = (releaseState: ReleaseState) =>
    run(() => setReleaseState.mutateAsync({ artifactId: artifact.id, releaseState }), "Artifact operation failed");

  const submitGrant = (event: FormEvent) => {
    event.preventDefault();
    if (!subjectId.trim()) return;
    void run(async () => {
      await grantAccess.mutateAsync({
        artifactId: artifact.id,
        subject: { kind: subjectKind, id: subjectId.trim() },
        level: grantLevel,
      });
      setSubjectId("");
    }, "Artifact operation failed");
  };

  const removeGrant = async (subject: { kind: "principal" | "group"; id: string }) => {
    const confirmed = await confirm({
      title: "Revoke access grant",
      body: <>Revoke {subject.kind} <strong>{subject.id}</strong> from <strong>{artifact.filename}</strong>?</>,
      confirmLabel: "Revoke",
      tone: "danger",
    });
    if (confirmed) {
      await run(() => revokeGrant.mutateAsync({ artifactId: artifact.id, subject }), "Artifact operation failed");
    }
  };

  const submitLink = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => {
      const max = maxDownloads ? Number.parseInt(maxDownloads, 10) : undefined;
      const created = await createLink.mutateAsync({
        artifactId: artifact.id,
        expiresAt: new Date(Date.now() + linkDays * 86_400_000).toISOString(),
        maxDownloads: max && max > 0 ? max : undefined,
      });
      setNewLink(created.url);
    }, "Share link creation failed");
  };

  const removeLink = async (linkId: string) => {
    const confirmed = await confirm({
      title: "Revoke share link",
      body: <>Revoke this bearer link? Anyone holding the URL immediately loses access.</>,
      confirmLabel: "Revoke",
      tone: "danger",
    });
    if (confirmed) {
      await run(() => revokeLink.mutateAsync({ artifactId: artifact.id, linkId }), "Artifact operation failed");
    }
  };

  const copyLink = async () => {
    if (!newLink) return;
    await navigator.clipboard.writeText(newLink);
    setLinkCopied(true);
    window.setTimeout(() => setLinkCopied(false), 1500);
  };

  return <DrawerShell title={artifact.filename} subtitle="Artifact" onClose={onClose} width="wide">
    <div className="drawer-body">
      <div className="drawer-status"><StatusPill value={artifact.releaseState} /><span>{formatBytes(artifact.byteLength)}</span><span>{artifact.mediaType}</span></div>
      {actionError && <div className="action-error">{actionError}</div>}
      <section className="artifact-preview-section">
        <div className="drawer-section-head">
          <h3>Preview</h3>
          <span className="subdued">Governed by the active Console session</span>
        </div>
        <ArtifactPreview
          key={`${artifact.id}:${artifact.authorizedGrants}`}
          artifact={artifact}
          principalId={principalId}
        />
      </section>
      {artifact.recording && (
        <section>
          <div className="recording-artifact-callout">
            <FileStack size={22} />
            <div>
              <strong>{recordingRelationTitle}</strong>
              <span>
                {artifact.recording.kind.replaceAll("_", " ")} · open the complete ordered capture
                in the Rerun workspace.
              </span>
            </div>
            <button className="button button-primary" onClick={() => onOpenRecording(artifact.recording!.recordingId)}>Open recording</button>
          </div>
        </section>
      )}
      <section>
        <h3>Identity</h3>
        <button className="copy-field" onClick={() => void copyId()}><span className="mono">artifact://{artifact.id}</span>{copied ? <Check size={15} /> : <Copy size={15} />}</button>
        <dl className="definition-list compact"><div><dt>Owner</dt><dd>{artifact.owner}</dd></div><div><dt>Owner identity</dt><dd className="mono">{artifact.outputOwner.kind}:{artifact.outputOwner.id}</dd></div><div><dt>Created</dt><dd>{formatDate(artifact.createdAt)}</dd></div><div><dt>Retention</dt><dd>{formatDate(artifact.retentionExpiresAt)}</dd></div></dl>
      </section>
      <section>
        <div className="drawer-section-head"><h3>Effective access</h3><StatusPill value={artifact.effectiveAccess.level ?? "denied"} /></div>
        <div className={`effective-access-card ${artifact.effectiveAccess.read ? "effective-access-allowed" : "effective-access-denied"}`}>
          <ShieldCheck size={21} />
          <div>
            <strong>{artifact.effectiveAccess.read ? `${artifact.effectiveAccess.level} access is effective` : "Read access is not effective"}</strong>
            <span>
              {artifact.effectiveAccess.read
                ? `${artifact.effectiveAccess.sources.length} authority source${artifact.effectiveAccess.sources.length === 1 ? "" : "s"} contribute to this decision.`
                : artifact.effectiveAccess.denialReason === "clearance"
                  ? "Required data labels exceed this principal’s clearance."
                  : artifact.effectiveAccess.denialReason === "need_to_know"
                    ? "Clearance is satisfied, but no direct, group, or Work Context authority grants read access."
                    : "The artifact belongs to another tenant boundary."}
            </span>
          </div>
        </div>
        {!!artifact.effectiveAccess.sources.length && <div className="access-source-list">{artifact.effectiveAccess.sources.map((source) => <div key={`${source.kind}:${source.subject}`}><span className="code-label">{source.kind.replaceAll("_", " ")}</span><strong>{source.subject}</strong><span>{source.level}</span></div>)}</div>}
      </section>
      <section>
        <div className="drawer-section-head"><h3>Provenance</h3><Fingerprint size={17} /></div>
        <dl className="definition-list compact">
          <div><dt>Work Context</dt><dd>{artifact.provenance.workContext}</dd></div>
          <div><dt>Producer</dt><dd className="mono">{artifact.provenance.producer}</dd></div>
          <div><dt>Invocation</dt><dd>{artifact.provenance.invocationMode}</dd></div>
          <div><dt>Initiator</dt><dd className="mono">{artifact.provenance.initiator ?? "-"}</dd></div>
          <div><dt>Delegation</dt><dd className="mono">{artifact.provenance.delegationId ?? "-"}</dd></div>
          <div><dt>Policy revision</dt><dd className="mono">{artifact.provenance.policyRevision}</dd></div>
        </dl>
      </section>
      {canAdminister && <section>
        <h3>Release state</h3>
        <div className="segmented" role="group" aria-label="Artifact release state">
          {(["private", "releasable", "released"] as const).map((state) => <button key={state} className={artifact.releaseState === state ? "segment-active" : ""} disabled={pending || artifact.releaseState === state} onClick={() => void changeReleaseState(state)}>{state}</button>)}
        </div>
      </section>}
      <section>
        <div className="drawer-section-head"><h3>Authorized access</h3><span className="subdued">{artifact.authorizedGrants} grants</span></div>
        {canAdminister && <form className="inline-form" onSubmit={submitGrant}>
          <select value={subjectKind} onChange={(event) => setSubjectKind(event.target.value as "principal" | "group")} aria-label="Grant subject type"><option value="principal">User</option><option value="group">Group</option></select>
          <input value={subjectId} onChange={(event) => setSubjectId(event.target.value)} placeholder="Principal or group ID" aria-label="Grant subject ID" required />
          <select value={grantLevel} onChange={(event) => setGrantLevel(event.target.value as "read" | "write" | "admin")} aria-label="Grant permission"><option value="read">Read</option><option value="write">Write</option><option value="admin">Admin</option></select>
          <button className="icon-button" type="submit" title="Add grant" disabled={pending || !subjectId.trim()}><UserRound size={15} /></button>
        </form>}
        <div className="access-list">
          {artifact.grants.map((grant) => <div className="access-row" key={`${grant.subjectKind}:${grant.subject}`}><div><strong>{grant.subject}</strong><span>{grant.subjectKind} · {grant.permission}</span></div>{canAdminister && <button className="icon-button icon-danger" title="Revoke grant" disabled={pending} onClick={() => void removeGrant({ kind: grant.subjectKind, id: grant.subject })}><Trash2 size={14} /></button>}</div>)}
          {!artifact.grants.length && <div className="access-empty">No projected grants</div>}
        </div>
      </section>
      <section>
        <div className="drawer-section-head"><h3>Anyone with link</h3><span className="subdued">{artifact.activeLinks} active</span></div>
        {newLink && <div className="one-time-link"><span>New link · shown once</span><button className="copy-field" onClick={() => void copyLink()}><span className="mono">{newLink}</span>{linkCopied ? <Check size={15} /> : <Copy size={15} />}</button></div>}
        {canAdminister && <form className="inline-form share-form" onSubmit={submitLink}>
          <label><span>Days</span><input type="number" min="1" max="30" value={linkDays} onChange={(event) => setLinkDays(Number(event.target.value))} /></label>
          <label><span>Download limit</span><input type="number" min="1" value={maxDownloads} onChange={(event) => setMaxDownloads(event.target.value)} placeholder="Unlimited" /></label>
          <button className="button button-secondary" type="submit" disabled={pending || artifact.releaseState === "private"}><Link2 size={14} /> Create link</button>
        </form>}
        <div className="access-list">
          {artifact.shareLinks.map((link) => <div className="access-row" key={link.id}><div><strong>{link.active ? "Active bearer link" : "Inactive bearer link"}</strong><span>{formatDate(link.expiresAt)} · {link.downloadCount}{link.maxDownloads ? ` / ${link.maxDownloads}` : ""} downloads</span></div>{canAdminister && link.active && <button className="icon-button icon-danger" title="Revoke link" disabled={pending} onClick={() => void removeLink(link.id)}><Trash2 size={14} /></button>}</div>)}
          {!artifact.shareLinks.length && <div className="access-empty">No share links</div>}
        </div>
      </section>
    </div>
    <footer className="drawer-footer">{artifact.effectiveAccess.read ? <a className="button button-secondary" href={artifactDownloadUrl(artifact.id)}><Download size={15} /> Download original</a> : <button className="button button-secondary" disabled><Download size={15} /> Download requires read access</button>}</footer>
  </DrawerShell>;
}
