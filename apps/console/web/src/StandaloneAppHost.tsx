import { useCallback, useEffect, useState } from "react";
import { LayoutGrid } from "lucide-react";
import { AppFrame } from "./apps/AppFrame";
import { resolveAppLink } from "./apps/links";
import { initializeAppSession } from "./api";
import { requestStandaloneBootstrap } from "./standaloneBootstrap";
import type { AppDescriptor } from "./types";

export function StandaloneAppHost() {
  const [app, setApp] = useState<AppDescriptor>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    const abort = new AbortController();
    void requestStandaloneBootstrap(window.location.pathname, (input, init) =>
      fetch(input, { ...init, signal: abort.signal }),
    )
      .then((bootstrap) => {
        initializeAppSession(bootstrap.csrfToken);
        setApp(bootstrap.app);
        document.title = bootstrap.app.title ?? bootstrap.app.name;
      })
      .catch((cause: unknown) => {
        if (abort.signal.aborted) return;
        setError(cause instanceof Error ? cause.message : "Standalone MCP App failed to open");
      });
    return () => abort.abort();
  }, []);

  const openInternalLink = useCallback((url: string) => {
    if (!app) return false;
    const target = resolveAppLink(url, [app]);
    if (target?.kind === "app") {
      window.location.assign(target.app.standalonePath);
      return true;
    }
    if (target?.kind === "platform") {
      window.location.assign(`/console/#/${target.view}`);
      return true;
    }
    return false;
  }, [app]);

  if (!app) {
    return (
      <main className="center-state error-state">
        {error ? <LayoutGrid size={30} /> : <div className="loading-mark" aria-label="Loading" />}
        {error && <><h1>App unavailable</h1><p>{error}</p></>}
      </main>
    );
  }

  return (
    <div className="standalone-app-shell">
      <header className="standalone-app-header">
        <h1>{app.title ?? app.name}</h1>
        <a className="button button-secondary" href="/console/#/apps">Return to Console</a>
      </header>
      <main className="standalone-app-content">
        <AppFrame app={app} onInternalLink={openInternalLink} />
      </main>
    </div>
  );
}
