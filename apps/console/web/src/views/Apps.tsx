import { LayoutGrid } from "lucide-react";
import { useCallback, useMemo } from "react";
import { EmptyState, SectionHeader } from "../components/primitives";
import { AppFrame } from "../apps/AppFrame";
import { appServerTitle, unavailableAppServers } from "../apps/catalogPresentation";
import { resolveAppLink, type PlatformAppLink } from "../apps/links";
import { isFullBleedApp } from "../appPresentation";
import { useApps } from "../queries";
import type { AppDescriptor } from "../types";

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
  const unavailableServers = unavailableAppServers(apps, degradations);
  if (!apps.length && !unavailableServers.length) {
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

  const selected = selectedUri
    ? apps.find((app) => app.resourceUri === selectedUri)
    : undefined;
  if (!selected) {
    return (
      <section className="panel full-panel">
        <SectionHeader title="Apps" count={apps.length + unavailableServers.length} />
        <p className="panel-intro">
          Interactive views shipped by hosted MCP servers, rendered in an isolated sandbox.
        </p>
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
          {unavailableServers.map((server) => (
            <div
              key={server}
              className="app-card app-card-unavailable"
              aria-label={`${appServerTitle(server)} unavailable`}
            >
              <LayoutGrid size={28} />
              <div className="app-card-heading">
                <strong>{appServerTitle(server)}</strong>
                <span className="app-unavailable-tag">Unavailable</span>
              </div>
              <span className="mono subdued">{server}</span>
            </div>
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
