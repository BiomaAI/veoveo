import { Fragment, useState } from "react";
import { ChevronDown, ChevronRight, Copy, Search } from "lucide-react";
import { EmptyState, SectionHeader, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import type { InstallationSnapshot } from "../types";

export function McpView({ snapshot }: { snapshot: InstallationSnapshot }) {
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<string>();
  const [copied, setCopied] = useState<string>();
  const rows = snapshot.servers.filter((server) =>
    [server.id, server.name, server.endpoint, ...server.tools, ...server.resources, ...server.prompts]
      .join(" ")
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const copyEndpoint = async (id: string, endpoint: string) => {
    await navigator.clipboard.writeText(endpoint);
    setCopied(id);
    window.setTimeout(() => setCopied(undefined), 1500);
  };
  return <section className="panel full-panel mcp-panel">
    <SectionHeader title="MCP servers" count={rows.length} actions={<label className="search-control"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search capabilities" /></label>} />
    <p className="panel-intro">Inspect the protocol surface exposed by each hosted server. Capability names and resource patterns come from the active gateway control plane.</p>
    <div className="table-scroll"><table className="mcp-table"><thead><tr><th aria-label="Expand" /><th>Server</th><th>State</th><th>Surface</th><th>Profiles</th><th>Transport</th><th>Endpoint</th></tr></thead><tbody>{rows.map((server) => {
      const isExpanded = expanded === server.id;
      const capabilityNames = Object.entries(server.capabilities).filter(([, enabled]) => enabled).map(([name]) => humanize(name));
      return <Fragment key={server.id}>
        <tr
          className="mcp-summary-row"
          onClick={() => setExpanded(isExpanded ? undefined : server.id)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              setExpanded(isExpanded ? undefined : server.id);
            }
          }}
          tabIndex={0}
          aria-expanded={isExpanded}
        >
          <td><span className="mcp-expander">{isExpanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}</span></td>
          <td><strong>{server.name}</strong><span className="mono subdued">{server.uriScheme}://</span></td>
          <td><StatusPill value={server.state} /></td>
          <td><strong>{server.tools.length} tools · {server.resources.length} resources</strong><span className="subdued">{server.prompts.length} prompts · {capabilityNames.length} protocol surfaces</span></td>
          <td><div className="tags">{server.profiles.map((profile) => <span key={profile}>{profile}</span>)}</div></td>
          <td><span className="code-label">{server.transport}</span></td>
          <td className="mono endpoint-cell">{server.endpoint}</td>
        </tr>
        {isExpanded && <tr className="mcp-detail-row"><td colSpan={7}><div className="mcp-detail">
          <div className="mcp-detail-head"><div><strong>{server.name} protocol surface</strong><span>Checked {formatDate(server.checkedAt)}</span></div><button className="button button-secondary" onClick={(event) => { event.stopPropagation(); void copyEndpoint(server.id, server.endpoint); }}><Copy size={14} /> {copied === server.id ? "Copied" : "Copy endpoint"}</button></div>
          <div className="mcp-capability-strip">{capabilityNames.map((capability) => <span key={capability}>{capability}</span>)}</div>
          <div className="mcp-detail-grid">
            <McpCapabilityList title="Tools" items={server.tools} empty="No tools exposed" />
            <McpCapabilityList title="Resources" items={server.resources} empty="No resources exposed" />
            <McpCapabilityList title="Prompts" items={server.prompts} empty="No prompts exposed" />
            <McpCapabilityList title="Required scopes" items={server.requiredScopes} empty="No server-level scopes" />
            {server.compatibilityHelpers.length > 0 && <McpCapabilityList title="Compatibility helpers" items={server.compatibilityHelpers} empty="" />}
            {server.ownedRoutes.length > 0 && <McpCapabilityList title="Owned HTTP routes" items={server.ownedRoutes.map((route) => `${route.path} · ${humanize(route.purpose)}`)} empty="" />}
          </div>
        </div></td></tr>}
      </Fragment>;
    })}</tbody></table></div>
    {!rows.length && <EmptyState>No MCP capability matches the current search.</EmptyState>}
  </section>;
}

function McpCapabilityList({ title, items, empty }: { title: string; items: string[]; empty: string }) {
  return <section className="mcp-capability-list"><h3>{title}<span>{items.length}</span></h3>{items.length ? <div>{items.map((item) => <code key={item}>{item}</code>)}</div> : <p>{empty}</p>}</section>;
}

function humanize(value: string) {
  return value.replace(/([a-z])([A-Z])/g, "$1 $2").replaceAll("_", " ").replace(/^./, (letter) => letter.toUpperCase());
}
