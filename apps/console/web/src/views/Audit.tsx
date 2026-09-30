import { useState, type FormEvent } from "react";
import { ChevronLeft, ChevronRight, Download } from "lucide-react";
import { SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import { parseAudit } from "../generatedContracts";
import type { AuditCursor, AuditPartition, AuditQuery, AuditTarget } from "../generated/audit";
import { AuditViewExpired, exportUrl } from "../audit/api";
import { partitionKey, useAuditLive, useAuditPartitions, useAuditRecords, useAuditSession } from "../audit/hooks";
import { activityLabel, classes, initialQuery, outcomes, partitionLabel, targetLabel, words } from "../audit/model";
import "../audit/audit.css";

export function AuditView() {
  const partitions = useAuditPartitions();
  const [selected, setSelected] = useState("");
  const partition = partitions.data?.find((value) => partitionKey(value) === selected) ?? partitions.data?.[0];
  if (partitions.error) return <p role="alert">{partitions.error.message}</p>;
  if (!partition) return <p>{partitions.isPending ? "Loading audit access…" : "No audit partitions are available."}</p>;
  return <>
    <label className="audit-partition">Audit partition
      <select value={partitionKey(partition)} onChange={(event) => setSelected(event.target.value)}>
        {partitions.data?.map((value) => <option key={partitionKey(value)} value={partitionKey(value)}>{partitionLabel(value)}</option>)}
      </select>
    </label>
    <PartitionAudit key={partitionKey(partition)} partition={partition} />
  </>;
}

function PartitionAudit({ partition }: { partition: AuditPartition }) {
  const [query, setQuery] = useState(() => initialQuery(partition));
  const [cursors, setCursors] = useState<(AuditCursor | null)[]>([null]);
  const [inputError, setInputError] = useState<string>();
  const session = useAuditSession(partition);
  const records = useAuditRecords({ ...query, cursor: cursors.at(-1) ?? null }, session.data);
  const live = useAuditLive(partition, session.data);
  const update = (next: AuditQuery) => { setQuery(next); setCursors([null]); setInputError(undefined); };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const value = (name: string) => String(form.get(name) ?? "").trim() || null;
    const date = (name: string) => { const raw = value(name); return raw ? new Date(raw).toISOString() : null; };
    try {
      const next = parseAudit("query", { ...query, cursor: null, class: value("class"),
        outcome: value("outcome"), actor: value("actor"), trace: value("trace"), from: date("from"), until: date("until") });
      if (next.from && next.until && next.from >= next.until) throw new Error("The end must follow the start.");
      update(next);
    } catch {
      setInputError("Check the filters. Trace IDs require 32 lowercase hexadecimal characters, and the end must follow the start.");
    }
  };
  const filterTarget = (target: AuditTarget | null) => update({ ...query, target });
  if (live === "denied") return <p role="alert">Your audit access has changed. Reload to check your permissions.</p>;
  if (live === "expired" || records.error instanceof AuditViewExpired || session.error) return <div role="alert"><p>{session.error?.message ?? "This audit view has expired."}</p><button className="button button-secondary" onClick={() => void session.refetch()}>Open audit view</button></div>;
  const page = records.error ? undefined : records.data;
  return <section className="panel full-panel">
    <SectionHeader title="Audit events" actions={<>
      <span className="subdued" role="status">{live === "live" ? "Live" : live === "connecting" ? "Connecting…" : "Reconnecting…"}</span>
      <a className="button button-secondary" href={exportUrl(query)}><Download size={15} /> Export JSONL</a>
    </>} />
    <form className="audit-filters" onSubmit={submit}>
      <label>Class<select name="class" defaultValue=""><option value="">All classes</option>{classes.map((value) => <option key={value} value={value}>{words(value)}</option>)}</select></label>
      <label>Outcome<select name="outcome" defaultValue=""><option value="">All outcomes</option>{outcomes.map((value) => <option key={value} value={value}>{words(value)}</option>)}</select></label>
      <label>Actor<input name="actor" placeholder="Principal ID" /></label>
      <label>Trace<input name="trace" placeholder="Trace ID" /></label>
      <label>From<input name="from" type="datetime-local" /></label>
      <label>Until<input name="until" type="datetime-local" /></label>
      <button className="button button-secondary" type="submit">Apply filters</button>
      <button className="button button-secondary" type="reset" onClick={() => update(initialQuery(partition))}>Clear</button>
    </form>
    {query.target && <div className="audit-notice">Target: {targetLabel(query.target)} <button className="button button-secondary" onClick={() => filterTarget(null)}>Clear target</button></div>}
    {inputError && <p className="audit-notice" role="alert">{inputError}</p>}
    {records.error && <div className="audit-notice" role="alert">{records.error.message} <button className="button button-secondary" onClick={() => void records.refetch()}>Retry</button></div>}
    <div className="table-scroll"><table><thead><tr><th>Time</th><th>Outcome</th><th>Actor</th><th>Activity</th><th>Target</th><th>Source</th><th>Trace</th></tr></thead>
      <tbody>{page?.records.map((record) => <tr key={record.id}>
        <td>{formatDate(record.occurredAt)}</td><td><StatusPill value={record.outcome} /><span className="audit-reason">{words(record.reason)}</span></td>
        <td>{record.actor ?? "Unauthenticated"}</td><td>{activityLabel(record.detail)}</td>
        <td><button className="audit-target" onClick={() => filterTarget(record.target)} title="Filter events for this target">{targetLabel(record.target)}</button></td>
        <td className="mono">{record.sourceIp ?? "—"}</td><td className="mono subdued">{record.traceId}</td>
      </tr>)}</tbody>
    </table></div>
    {records.isPending && <p className="audit-notice">Loading audit events…</p>}
    {page?.records.length === 0 && <p className="audit-notice">No events match these filters.</p>}
    <div className="pagination"><span>{page?.records.length ?? 0} events on this page{records.isFetching ? " · Updating…" : ""}</span><div>
      <button className="icon-button" aria-label="Previous audit page" disabled={cursors.length === 1 || records.isFetching} onClick={() => setCursors((values) => values.slice(0, -1))}><ChevronLeft size={15} /></button>
      <span>Page {cursors.length}</span>
      <button className="icon-button" aria-label="Next audit page" disabled={!page?.next || records.isFetching} onClick={() => { if (page?.next) setCursors((values) => [...values, page.next ?? null]); }}><ChevronRight size={15} /></button>
    </div></div>
  </section>;
}
