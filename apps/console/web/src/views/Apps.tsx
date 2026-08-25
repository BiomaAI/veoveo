import { LayoutGrid } from "lucide-react";
import { useCallback, useMemo } from "react";
import { EmptyState, SectionHeader } from "../components/primitives";
import { AppFrame } from "../apps/AppFrame";
import { resolveAppLink, type PlatformAppLink } from "../apps/links";
import { isFullBleedApp } from "../appPresentation";
import { useApps } from "../queries";
import type { AppCatalogDegradation, AppDescriptor } from "../types";

export function AppsView({
  selectedUri,
  onSelect,
  onPlatformSelect,
}: {
  selectedUri?: string;
  onSelect: (app: AppDescriptor) => void;
  onPlatformSelect: (view: PlatformAppLink) => void;
}) {
  const { data, error, isLoading } = useApps();
  const apps = useMemo(() => data?.apps ?? [], [data?.apps]);
  const openInternalLink = useCallback(
    (url: string) => {
      const target = resolveAppLink(url, apps);
      if (target?.kind === "app") {
        onSelect(target.app);
        return true;
      }
      if (target?.kind === "platform") {
        onPlatformSelect(target.view);
        return true;
      }
      return false;
    },
    [apps, onPlatformSelect, onSelect],
  );

  if (isLoading) {
    return (
      <section className="panel full-panel">
        <EmptyState>Loading MCP app catalog…</EmptyState>
      </section>
    );
  }
  const degradations = data?.degradations ?? [];
  const degradationNotice = degradations.length ? (
    <p className="catalog-degradation" role="status">
      Some hosted Apps are temporarily unavailable: {formatDegradations(degradations)}. Healthy
      Apps remain available.
    </p>
  ) : null;
  if (!apps.length) {
    const message =
      error instanceof Error
        ? error.message
        : "No hosted MCP server currently ships an app view.";
    return (
      <section className="panel full-panel">
        <SectionHeader title="Apps" />
        {degradationNotice}
        <EmptyState>{message}</EmptyState>
      </section>
    );
  }

  const selected = selectedUri
    ? apps.find((app) => app.resourceUri === selectedUri)
    : undefined;
  if (!selected) {
    return (
      <section className="panel full-panel">
        <SectionHeader title="Apps" count={apps.length} />
        <p className="panel-intro">
          Interactive views shipped by hosted MCP servers, rendered in an isolated sandbox.
        </p>
        {degradationNotice}
        <div className="app-catalog">
          {apps.map((app) => (
            <button key={app.resourceUri} className="app-card" onClick={() => onSelect(app)}>
              {app.icons?.[0] ? (
                <img src={app.icons[0]} alt="" width={28} height={28} />
              ) : (
                <LayoutGrid size={28} />
              )}
              <strong>{app.title ?? app.name}</strong>
              <span className="mono subdued">{app.server}</span>
              {app.description && <p>{app.description}</p>}
            </button>
          ))}
        </div>
      </section>
    );
  }

  const frame = (
    <AppFrame key={selected.resourceUri} app={selected} onInternalLink={openInternalLink} />
  );

  if (isFullBleedApp(selected)) {
    return (
      <section className="app-workspace">
        <div className="app-frame-panel app-frame-panel-fullbleed">{frame}</div>
      </section>
    );
  }

  return (
    <section className="panel full-panel">
      <SectionHeader title={selected.title ?? selected.name} />
      <p className="panel-intro">
        {selected.description ??
          "Interactive view shipped by the MCP server, rendered in an isolated sandbox."}{" "}
        Tools this app may call: {selected.tools.map((tool) => tool.name).join(", ") || "none"}.
      </p>
      <div className="app-frame-panel">{frame}</div>
    </section>
  );
}

function formatDegradations(degradations: AppCatalogDegradation[]): string {
  const servers = [...new Set(degradations.map((failure) => failure.server))];
  return servers.join(", ");
}
