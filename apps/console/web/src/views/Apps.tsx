import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { EmptyState, SectionHeader } from "../components/primitives";
import { AppFrame } from "../apps/AppFrame";
import { loadApps } from "../api";
import type { AppDescriptor } from "../types";

export function AppsView() {
  const { data, error, isLoading } = useQuery({
    queryKey: ["apps"] as const,
    queryFn: ({ signal }) => loadApps(signal),
  });
  const [selectedUri, setSelectedUri] = useState<string>();

  if (isLoading) {
    return (
      <section className="panel full-panel">
        <EmptyState>Loading MCP app catalog…</EmptyState>
      </section>
    );
  }
  const apps = data?.apps ?? [];
  if (!apps.length) {
    const message =
      error instanceof Error
        ? error.message
        : "No hosted MCP server currently ships an app view.";
    return (
      <section className="panel full-panel">
        <SectionHeader title="Apps" />
        <EmptyState>{message}</EmptyState>
      </section>
    );
  }
  const selected: AppDescriptor =
    apps.find((app) => app.resourceUri === selectedUri) ?? apps[0];

  return (
    <section className="panel full-panel">
      <SectionHeader
        title="Apps"
        count={apps.length}
        actions={
          apps.length > 1 ? (
            <label className="filter-control">
              <select
                value={selected.resourceUri}
                onChange={(event) => setSelectedUri(event.target.value)}
                aria-label="App"
              >
                {apps.map((app) => (
                  <option key={app.resourceUri} value={app.resourceUri}>
                    {app.server} · {app.title ?? app.name}
                  </option>
                ))}
              </select>
            </label>
          ) : undefined
        }
      />
      <p className="panel-intro">
        {selected.description ??
          "Interactive view shipped by the MCP server, rendered in an isolated sandbox."}{" "}
        Tools this app may call: {selected.tools.map((tool) => tool.name).join(", ") || "none"}.
      </p>
      <div className="app-frame-panel">
        <AppFrame key={selected.resourceUri} app={selected} />
      </div>
    </section>
  );
}
