import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Activity, ArrowLeft, Grid2X2, RefreshCw } from "lucide-react";
import type { AppCatalog, AppDescriptor } from "../../console/web/src/types.ts";
import { APP_FRAME_SANDBOX } from "../../console/web/src/apps/framePolicy.ts";
import { appCatalog } from "./appProtocol.ts";
import { appApi } from "./appApi.ts";
import { attachWorkspaceApp } from "./appBridge.ts";
import "./apps.css";

export function Apps({ chat, close, activity }: { chat: string; close: () => void; activity: () => void }) {
  const client = useQueryClient();
  const catalog = useQuery({ queryKey: ["apps"], queryFn: ({ signal }) => appApi.catalog(signal), refetchOnWindowFocus: true });
  const [uri, select] = useState<string | undefined>(() => new URLSearchParams(location.search).get("app") ?? undefined);
  const [connected, setConnected] = useState(false);
  useEffect(() => {
    const events = new EventSource("/workspace/api/apps/events");
    events.addEventListener("catalog", event => {
      try { client.setQueryData<AppCatalog>(["apps"], appCatalog.parse(JSON.parse((event as MessageEvent).data))); setConnected(true); }
      catch { setConnected(false); }
    });
    events.onerror = () => setConnected(false);
    return () => events.close();
  }, [client]);
  useEffect(() => {
    const previous = document.activeElement;
    return () => { if (previous instanceof HTMLElement && previous.isConnected) previous.focus(); };
  }, []);
  const admitted = JSON.stringify(catalog.data?.apps.map(app => app.resourceUri) ?? []);
  const navigate = useCallback((uri: string) => {
    if (!(JSON.parse(admitted) as string[]).includes(uri)) return false;
    select(uri); return true;
  }, [admitted]);
  const selected = catalog.data?.apps.find(app => app.resourceUri === uri);
  const changed = useCallback(() => {
    void client.invalidateQueries({ queryKey: ["operations"] });
    void client.invalidateQueries({ queryKey: ["operation"] });
  }, [client]);
  return <section className="workspace-apps" aria-label="Apps">
    <header className="apps-toolbar"><button autoFocus onClick={close}><ArrowLeft size={16}/> Back to chat</button><div><h2>{selected?.title ?? selected?.name ?? "Your apps"}</h2><p>Work you start here appears in your Activity. Chat members get no automatic access.</p></div>
      <button onClick={activity}><Activity size={16}/> Activity</button>{selected && <button onClick={() => select(undefined)}><Grid2X2 size={16}/> All apps</button>}
    </header>
    {!connected && <p className="muted" role="status">Connecting to the current app catalog…</p>}
    {!!catalog.data?.degradations.length && <p className="muted">Some services are unavailable. Available apps remain usable.</p>}
    {catalog.error && <p role="alert" className="error">{catalog.error.message} <button onClick={() => void catalog.refetch()}><RefreshCw size={14}/> Retry</button></p>}
    {selected ? <Frame key={selected.resourceUri} serialized={JSON.stringify(selected)} chat={chat} changed={changed} navigate={navigate}/> : <div className="apps-grid">
      {catalog.data?.apps.map(app => <button className="app-choice" key={app.resourceUri} onClick={() => select(app.resourceUri)}><Grid2X2 size={20}/><strong>{app.title ?? app.name}</strong><span>{app.description}</span></button>)}
      {catalog.isPending && <p role="status">Loading your apps…</p>}
      {catalog.data?.apps.length === 0 && <p>No apps are available with your current access.</p>}
      {uri && !selected && !catalog.isPending && <p>This app is no longer available with your access.</p>}
    </div>}
  </section>;
}

function Frame({ serialized, chat, changed, navigate }: { serialized: string; chat: string; changed: () => void; navigate: (uri: string) => boolean }) {
  const app = useMemo(() => JSON.parse(serialized) as AppDescriptor, [serialized]);
  const ref = useRef<HTMLIFrameElement>(null);
  const navigation = useRef(navigate);
  const [error, setError] = useState<string>();
  useEffect(() => { navigation.current = navigate; }, [navigate]);
  useEffect(() => { if (ref.current) return attachWorkspaceApp(ref.current, app, chat, changed, uri => navigation.current(uri), setError); }, [app, chat, changed]);
  return <>{error && <p className="error" role="alert">{error}</p>}<iframe ref={ref} className="workspace-app-frame" title={app.title ?? app.name} sandbox={APP_FRAME_SANDBOX} referrerPolicy="no-referrer"
    src={`/workspace/api/apps/frame?uri=${encodeURIComponent(app.resourceUri)}`}/></>;
}
