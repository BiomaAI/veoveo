import { useState } from "react";
import { SectionHeader } from "../components/primitives";
import type { AuditDailyQuery, AuditPartition } from "../generated/audit";
import { partitionKey, useAuditDaily, useAuditLive, useAuditPartitions, useAuditSession } from "./hooks";
import { AuditViewExpired } from "./api";
import { outcomes, partitionLabel, words } from "./model";
import "./audit.css";

export function AuditOverview() {
  const partitions = useAuditPartitions();
  const partition = partitions.data?.[0];
  return <section className="panel">
    <SectionHeader title="Audit activity" />
    {partitions.error ? <p className="audit-notice" role="alert">{partitions.error.message}</p> :
      partition ? <DailyCounts key={partitionKey(partition)} partition={partition} /> :
        <p className="audit-notice">{partitions.isPending ? "Loading audit access…" : "No audit access."}</p>}
  </section>;
}
function DailyCounts({ partition }: { partition: AuditPartition }) {
  const [query] = useState<AuditDailyQuery>(() => {
    const today = new Date(); today.setUTCHours(0, 0, 0, 0);
    return { partition, from: new Date(today.getTime() - 29 * 86400_000).toISOString(),
      until: new Date(today.getTime() + 86400_000).toISOString(), cursor: null, limit: 1000 };
  });
  const session = useAuditSession(partition);
  const daily = useAuditDaily(query, session.data);
  const live = useAuditLive(partition, session.data);
  if (live === "denied") return <p className="audit-notice" role="alert">Audit access has changed.</p>;
  if (live === "expired" || daily.error instanceof AuditViewExpired || session.error) return <div className="audit-notice" role="alert"><p>{session.error?.message ?? "This audit view has expired."}</p><button className="button button-secondary" onClick={() => void session.refetch()}>Open audit view</button></div>;
  if (daily.error) return <p className="audit-notice" role="alert">{daily.error.message}</p>;
  if (!daily.data) return <p className="audit-notice">Loading daily counts…</p>;
  // Thirty UTC days, six classes and four outcomes fit in one 1,000-row page.
  // Never show a partial total if the contract's grouping gains another dimension.
  if (daily.data.next) return <p className="audit-notice" role="alert">The daily summary exceeds this view's range.</p>;
  const totals = { allowed: 0, denied: 0, succeeded: 0, failed: 0 };
  for (const count of daily.data.counts) {
    totals[count.outcome] += count.count;
    if (!Number.isSafeInteger(totals[count.outcome])) return <p className="audit-notice" role="alert">Daily counts exceed the supported display range.</p>;
  }
  return <div className="audit-daily">
    <p>{partitionLabel(partition)} · 30 UTC days through {query.until ? new Date(new Date(query.until).getTime() - 1).toISOString().slice(0, 10) : "today"}</p>
    <dl>{outcomes.map((outcome) => <div key={outcome}><dt>{words(outcome)}</dt><dd>{totals[outcome].toLocaleString()}</dd></div>)}</dl>
    <span className="subdued">{live === "live" ? "Live updates" : "Reconnecting for updates…"}</span>
  </div>;
}
