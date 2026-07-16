import { useState } from "react";
import { ChevronLeft, ChevronRight, Download, Search } from "lucide-react";
import { SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import type { InstallationSnapshot } from "../types";

export function AuditView({ snapshot }: { snapshot: InstallationSnapshot }) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const pageSize = 25;
  const rows = snapshot.audit.filter((event) => `${event.actor} ${event.action} ${event.resource} ${event.outcome}`.toLowerCase().includes(query.toLowerCase()));
  const pages = Math.max(1, Math.ceil(rows.length / pageSize));
  const currentPage = Math.min(page, pages - 1);
  const visibleRows = rows.slice(currentPage * pageSize, (currentPage + 1) * pageSize);
  const updateQuery = (value: string) => { setQuery(value); setPage(0); };
  const exportAudit = () => {
    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`;
    const csv = [
      ["time", "outcome", "actor", "action", "resource", "source_ip", "trace_id"].join(","),
      ...rows.map((event) => [event.occurredAt, event.outcome, event.actor, event.action, event.resource, event.sourceIp ?? "", event.traceId ?? ""].map(quote).join(",")),
    ].join("\n");
    const url = URL.createObjectURL(new Blob([csv], { type: "text/csv;charset=utf-8" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = `veoveo-audit-${new Date().toISOString()}.csv`;
    link.click();
    URL.revokeObjectURL(url);
  };
  return <section className="panel full-panel">
    <SectionHeader
      title="Audit events"
      count={rows.length}
      actions={<>
        <label className="search-control"><Search size={15} /><input value={query} onChange={(event) => updateQuery(event.target.value)} placeholder="Search audit" /></label>
        <button className="button button-secondary" onClick={exportAudit}><Download size={15} /> Export</button>
      </>}
    />
    <div className="table-scroll"><table><thead><tr><th>Time</th><th>Outcome</th><th>Actor</th><th>Action</th><th>Resource</th><th>Source</th><th>Trace</th></tr></thead><tbody>{visibleRows.map((event) => <tr key={event.id}><td>{formatDate(event.occurredAt)}</td><td><StatusPill value={event.outcome} /></td><td>{event.actor}</td><td className="mono">{event.action}</td><td>{event.resource}</td><td className="mono">{event.sourceIp ?? "-"}</td><td className="mono subdued">{event.traceId ?? "-"}</td></tr>)}</tbody></table></div>
    <div className="pagination">
      <span>{rows.length ? `${currentPage * pageSize + 1}–${Math.min((currentPage + 1) * pageSize, rows.length)} of ${rows.length}` : "0 events"}</span>
      <div>
        <button className="icon-button" aria-label="Previous audit page" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}><ChevronLeft size={15} /></button>
        <span>Page {currentPage + 1} of {pages}</span>
        <button className="icon-button" aria-label="Next audit page" disabled={currentPage + 1 >= pages} onClick={() => setPage(currentPage + 1)}><ChevronRight size={15} /></button>
      </div>
    </div>
  </section>;
}
