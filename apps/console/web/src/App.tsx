import { useConsoleBootstrap, consoleIdentityScope } from "./bootstrap";
import { createConsoleQueryClient } from "./queryClient";
import type { ConsoleBootstrap } from "./generated/console";
import { ComputersPage } from "./computers/ComputersPage";
import { Fragment, useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import {
  Activity,
  Archive,
  Bot,
  Boxes,
  ChevronRight,
  FileStack,
  Gauge,
  KeyRound,
  LayoutGrid,
  LogOut,
  Menu,
  Monitor,
  Network,
  Palette,
  ShieldCheck,
  UserRound,
  Users,
  X
} from "lucide-react";
import { QueryClientProvider, useQueryClient } from "@tanstack/react-query";
import { loadArtifact, logoutConsole } from "./api";
import { UploadQueue, type QueueState } from "./uploads/queue";
import { UploadPanel } from "./uploads/UploadPanel";
import type { Receipt } from "./uploads/model";
import { useAppCatalogLive, useConsoleLiveStream } from "./live";
import { queryKeys, useApps, useSnapshot } from "./queries";
import { Overview } from "./views/Overview";
import { WorkView } from "./views/Work";
import { ArtifactsView } from "./views/Artifacts";
import { AgentsView } from "./views/Agents";
import { RecordingsView } from "./views/Recordings";
import { McpView } from "./views/Mcp";
import { AppsView } from "./views/Apps";
import { AccessView } from "./views/Access";
import { AuditView } from "./views/Audit";
import { ClusterView } from "./views/Cluster";
import { ArtifactDrawer } from "./drawers/ArtifactDrawer";
import { TaskDrawer } from "./drawers/TaskDrawer";
import type { AppDescriptor, ArtifactSummary, InstallationSnapshot, TaskSummary } from "./types";
import { consoleThemes, useTheme, type ConsoleTheme } from "./theme";
import { isFullBleedApp } from "./appPresentation";
import { groupAppsByServer, namespacedAppTitle } from "./apps/catalogPresentation";

// Core Computers has an accepted native projection. Other domain pages come from the MCP App catalog.
const navItems = [
  { id: "computers", label: "Computers", icon: Monitor },
  { id: "overview", label: "Overview", icon: Gauge },
  { id: "work", label: "Work", icon: Activity },
  { id: "artifacts", label: "Artifacts", icon: Archive },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "recordings", label: "Recordings", icon: FileStack },
  { id: "mcp", label: "MCP", icon: Network },
  { id: "apps", label: "Apps", icon: LayoutGrid },
  { id: "access", label: "Access", icon: ShieldCheck },
  { id: "audit", label: "Audit", icon: KeyRound },
  { id: "cluster", label: "Cluster", icon: Boxes }
] as const;

const OPEN_APP_SERVERS_KEY = "veoveo.console.open-app-servers";
const EMPTY_UPLOAD_QUEUE: QueueState = { entries: [] };
const noUploadSubscription = () => () => {};

function storedOpenAppServers(): Set<string> {
  try {
    const value = JSON.parse(window.localStorage.getItem(OPEN_APP_SERVERS_KEY) ?? "[]");
    return new Set(Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : []);
  } catch {
    return new Set();
  }
}

type ViewId = (typeof navItems)[number]["id"];

function appRoute(resourceUri: string): string {
  return `#/apps/${resourceUri.replace(/^ui:\/\//, "")}`;
}

function initialRoute(): { view: ViewId; recordingId?: string; appUri?: string } {
  const [value, ...rest] = window.location.hash.replace(/^#\/?/, "").split("/");
  const view = navItems.some((item) => item.id === value) ? (value as ViewId) : "computers";
  return {
    view,
    recordingId: view === "recordings" && rest[0] ? rest[0] : undefined,
    appUri: view === "apps" && rest.length >= 2 ? `ui://${rest.join("/")}` : undefined,
  };
}

function logoSource(logo: string): string {
  return logo.startsWith("data:") ? logo : `data:image/svg+xml;utf8,${encodeURIComponent(logo)}`;
}

export function App() {
  const bootstrap = useConsoleBootstrap();
  if (bootstrap.isLoading) return <div className="center-state"><div className="loading-mark" aria-label="Loading session" /></div>;
  if (!bootstrap.data) return <div className="center-state error-state"><h1>Console unavailable</h1>
    <p>{bootstrap.error instanceof Error ? bootstrap.error.message : "The session could not be loaded."}</p>
    <button className="button button-primary" onClick={() => void bootstrap.refetch()}>Retry</button>
    <a className="button button-secondary" href="/auth/login">Sign in</a></div>;
  return <ScopedConsole key={consoleIdentityScope(bootstrap.data)} bootstrap={bootstrap.data} />;
}

function ScopedConsole({ bootstrap }: { bootstrap: ConsoleBootstrap }) {
  const [client] = useState(createConsoleQueryClient);
  useEffect(() => () => { void client.cancelQueries(); client.clear(); }, [client]);
  return <QueryClientProvider client={client}><Console bootstrap={bootstrap} /></QueryClientProvider>;
}

function Console({ bootstrap }: { bootstrap: ConsoleBootstrap }) {
  const initial = initialRoute();
  const { theme, setTheme } = useTheme();
  const queryClient = useQueryClient();
  const inventory = useSnapshot(bootstrap.canReadInstallation);
  const snapshot = bootstrap.canReadInstallation ? inventory.data : undefined;
  const { data: appsCatalog } = useApps();
  useAppCatalogLive(true);
  const [selectedView, setView] = useState<ViewId>(initial.view);
  const view = bootstrap.canReadInstallation || selectedView === "computers" || selectedView === "apps" ? selectedView : "computers";
  const [selectedAppUri, setSelectedAppUri] = useState<string | undefined>(initial.appUri);
  const [mobileNav, setMobileNav] = useState(false);
  const [artifactSelection, setArtifactSelection] = useState<{ artifact: ArtifactSummary; scope: string }>();
  const [uploadsOpen, setUploadsOpen] = useState(false);
  const [selectedTask, setSelectedTask] = useState<TaskSummary>();
  const [selectedRecordingId, setSelectedRecordingId] = useState<string | undefined>(initial.recordingId);
  const [signOutError, setSignOutError] = useState<string>();
  const [signingOut, setSigningOut] = useState(false);
  const [openAppServers, setOpenAppServers] = useState(storedOpenAppServers);
  const actor = bootstrap.session.actorId;
  const workContext = bootstrap.session.workContext;
  const tenant = bootstrap.session.tenantId;
  const artifactScope = JSON.stringify([tenant, actor, workContext]);
  const selectedArtifact = artifactSelection?.scope === artifactScope ? artifactSelection.artifact : undefined;
  const setSelectedArtifact = (artifact?: ArtifactSummary) => setArtifactSelection(artifact ? { artifact, scope: artifactScope } : undefined);
  const receiveUpload = useCallback((receipt: Receipt) => {
    queryClient.setQueryData<InstallationSnapshot>(queryKeys.snapshot, (current) => current && ({
      ...current, artifacts: current.artifacts.map((artifact) => artifact.id === receipt.artifact_id ? {
        ...artifact, byteLength: receipt.byte_len, filename: receipt.filename, mediaType: receipt.mime_type,
      } : artifact),
    }));
    void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
  }, [queryClient]);
  const uploadQueue = useMemo(() => actor && workContext && tenant ? new UploadQueue(actor, workContext, tenant, receiveUpload) : undefined, [actor, workContext, tenant, receiveUpload]);
  const liveStatus = useConsoleLiveStream(snapshot?.stream.cursor, uploadQueue?.reconcile);
  useEffect(() => {
    if (!uploadQueue) return;
    void uploadQueue.initialize();
    return () => uploadQueue.dispose();
  }, [uploadQueue]);
  const uploadState = useSyncExternalStore(uploadQueue?.subscribe ?? noUploadSubscription, uploadQueue?.snapshot ?? (() => EMPTY_UPLOAD_QUEUE));
  const viewUpload = async (receipt: Receipt) => {
    const artifact = await loadArtifact(receipt.artifact_id);
    setSelectedArtifact(artifact); setUploadsOpen(false);
  };
  const apps = useMemo(() => appsCatalog?.apps ?? [], [appsCatalog?.apps]);
  const appGroups = useMemo(
    () => groupAppsByServer(apps, appsCatalog?.degradations ?? []),
    [apps, appsCatalog?.degradations],
  );
  const selectedApp = selectedAppUri
    ? apps.find((app) => app.resourceUri === selectedAppUri)
    : undefined;

  const navigate = useCallback((next: ViewId, recordingId?: string) => {
    setView(next);
    setSelectedRecordingId(recordingId);
    setSelectedAppUri(undefined);
    setMobileNav(false);
    window.history.replaceState(
      null,
      "",
      recordingId ? `#/${next}/${encodeURIComponent(recordingId)}` : `#/${next}`
    );
  }, []);

  const navigateApp = useCallback((app: AppDescriptor) => {
    setView("apps");
    setSelectedAppUri(app.resourceUri);
    setSelectedRecordingId(undefined);
    setMobileNav(false);
    window.history.replaceState(null, "", appRoute(app.resourceUri));
  }, []);

  const retrySnapshot = () => void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });

  const installation = bootstrap.installation;
  useEffect(() => {
    if (!installation) return;
    document.title = `${installation.name} Console`;
    if (installation.accentColor) {
      document.documentElement.style.setProperty("--brand-accent", installation.accentColor);
    }
    if (installation.logo) {
      const icon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
      if (icon) icon.href = logoSource(installation.logo);
    }
  }, [installation]);

  useEffect(() => {
    window.localStorage.setItem(OPEN_APP_SERVERS_KEY, JSON.stringify([...openAppServers].sort()));
  }, [openAppServers]);

  const signOut = async () => {
    uploadQueue?.pauseAll(true);
    setSigningOut(true);
    try {
      await logoutConsole();
    } catch (cause) {
      setSignOutError(cause instanceof Error ? cause.message : "Sign out failed");
      setSigningOut(false);
    }
  };

  const title =
    view === "apps" && selectedApp
      ? namespacedAppTitle(selectedApp)
      : navItems.find((item) => item.id === view)?.label ?? "Overview";
  const currentArtifact = selectedArtifact && (snapshot?.artifacts.find((item) => item.id === selectedArtifact.id) ?? selectedArtifact);
  const currentTask = selectedTask && snapshot?.tasks.find((item) => item.id === selectedTask.id);
  const accountName = bootstrap.session.displayName.trim();

  return (
    <><div className="app-shell">
      <aside className={`sidebar ${mobileNav ? "sidebar-open" : ""}`}>
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            {bootstrap.installation.logo
              ? <img src={logoSource(bootstrap.installation.logo)} alt="" />
              : bootstrap.installation.name.charAt(0).toUpperCase()}
          </div>
          <div>
            <strong>{bootstrap.installation.name}</strong>
            <span>{bootstrap.installation.productLabel}</span>
          </div>
          <button className="icon-button mobile-close" onClick={() => setMobileNav(false)} title="Close navigation">
            <X size={18} />
          </button>
        </div>
        <nav aria-label="Primary navigation">
          {navItems.filter((item) => bootstrap.canReadInstallation || item.id === "computers" || item.id === "apps").map(({ id, label, icon: Icon }) => (
            <Fragment key={id}>
              <button
                className={view === id && !(id === "apps" && selectedApp) ? "nav-active" : ""}
                onClick={() => navigate(id)}
              >
                <Icon size={17} />
                <span>{label}</span>
              </button>
              {id === "apps" &&
                appGroups.map((group) => (
                  <details
                    key={group.server}
                    className="nav-app-group"
                    open={openAppServers.has(group.server) || selectedApp?.server === group.server}
                    onToggle={(event) => {
                      const open = event.currentTarget.open;
                      setOpenAppServers((current) => {
                        const next = new Set(current);
                        if (open) next.add(group.server);
                        else next.delete(group.server);
                        return next;
                      });
                    }}
                  >
                    <summary className={selectedApp?.server === group.server ? "nav-app-server-active" : ""}>
                      <ChevronRight size={14} className="nav-app-chevron" />
                      <span>{group.title}</span>
                      {group.unavailable && <span className="nav-app-unavailable">Unavailable</span>}
                    </summary>
                    {group.apps.map((app) => (
                      <button
                        key={app.resourceUri}
                        className={`nav-app ${view === "apps" && selectedApp?.resourceUri === app.resourceUri ? "nav-active" : ""}`}
                        onClick={() => navigateApp(app)}
                      >
                        {app.icons?.[0] ? (
                          <img src={app.icons[0]} alt="" width={17} height={17} />
                        ) : (
                          <LayoutGrid size={17} />
                        )}
                        <span>{app.title ?? app.name}</span>
                      </button>
                    ))}
                  </details>
                ))}
            </Fragment>
          ))}
        </nav>
        {snapshot && <div className="sidebar-foot">
          <div className={`live-dot ${liveStatus === "reconnecting" || snapshot.services.some((service) => service.state === "offline") ? "live-off" : ""}`} />
          <div>
            <strong>{liveStatus === "live" ? "Live" : liveStatus === "reconnecting" ? "Reconnecting" : "Status"}</strong>
            <span>{snapshot.services.filter((service) => service.state === "healthy").length}/{snapshot.services.length} platform services healthy</span>
          </div>
        </div>}
      </aside>

      {mobileNav && <button className="nav-scrim" aria-label="Close navigation" onClick={() => setMobileNav(false)} />}

      <div className="main-shell">
        <header className="topbar">
          <div className="topbar-title">
            <button className="icon-button mobile-menu" onClick={() => setMobileNav(true)} title="Open navigation">
              <Menu size={19} />
            </button>
            <div>
              <span>{bootstrap.installation.name}</span>
              <h1>{title}</h1>
            </div>
          </div>
          <div className="topbar-actions">
            {uploadQueue && <button className="button button-secondary" onClick={() => setUploadsOpen(true)} aria-label="Open uploads">
              Uploads{uploadState.entries.length ? ` (${uploadState.entries.filter((entry) => !["Ready", "Cancelled"].includes(entry.phase)).length} active · ${uploadState.entries.filter((entry) => entry.phase === "Ready").length} ready)` : ""}
            </button>}
            <label className="theme-select" title="Console theme">
              <Palette size={15} />
              <select
                value={theme}
                onChange={(event) => setTheme(event.target.value as ConsoleTheme)}
                aria-label="Console theme"
              >
                {consoleThemes.map((candidate) => (
                  <option key={candidate.id} value={candidate.id}>
                    {candidate.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="tenant-select">
              <Users size={15} />
              <select value={bootstrap.session.tenantId} aria-label="Tenant" disabled={bootstrap.session.availableTenants.length <= 1}>
                {bootstrap.session.availableTenants.map((tenant) => <option key={tenant.id} value={tenant.id}>{tenant.name}</option>)}
              </select>
            </label>
            <div
              className="user-menu"
              title={`Signed in as ${accountName}`}
            >
              <span>
                {accountName.split(/\s+/).map((part) => part[0]).join("").slice(0, 2) || <UserRound size={14} />}
              </span>
              <strong>{accountName}</strong>
            </div>
            <button className="icon-button" onClick={() => void signOut()} title="Sign out" disabled={signingOut}>
              <LogOut size={17} />
            </button>
          </div>
        </header>

        <main
          className={
            view === "recordings"
              ? "content content-recordings"
              : view === "apps" && isFullBleedApp(selectedApp)
                ? "content content-app-fullbleed"
                : "content"
          }
        >
          {signOutError && <p role="alert">{signOutError}</p>}
          {view === "computers" && <ComputersPage scope={consoleIdentityScope(bootstrap)} canReadInstallation={bootstrap.canReadInstallation}
            artifacts={snapshot?.artifacts ?? []} uploads={uploadState} onUpload={() => setUploadsOpen(true)} />}
          {view !== "computers" && view !== "apps" && !snapshot && <div className="center-state">
            <p>{inventory.isLoading ? "Loading installation…" : inventory.error instanceof Error ? inventory.error.message : "Installation inventory is unavailable."}</p>
            <button className="button button-secondary" onClick={retrySnapshot}>Retry</button></div>}
          {snapshot && view === "overview" && <Overview snapshot={snapshot} onArtifact={setSelectedArtifact} onTask={setSelectedTask} />}
          {snapshot && view === "work" && <WorkView tasks={snapshot.tasks} onSelect={setSelectedTask} />}
          {snapshot && view === "artifacts" && <ArtifactsView artifacts={snapshot.artifacts} onSelect={setSelectedArtifact} onUpload={() => setUploadsOpen(true)} />}
          {snapshot && view === "agents" && <AgentsView snapshot={snapshot} />}
          {snapshot && view === "recordings" && <RecordingsView snapshot={snapshot} initialRecordingId={selectedRecordingId} onRecordingSelect={(recordingId) => {
            setSelectedRecordingId(recordingId);
            window.history.replaceState(null, "", `#/recordings/${encodeURIComponent(recordingId)}`);
          }} />}
          {snapshot && view === "mcp" && <McpView snapshot={snapshot} />}
          {view === "apps" && (
            <AppsView
              selectedUri={selectedAppUri}
              onSelect={navigateApp}
              onPlatformSelect={navigate}
            />
          )}
          {snapshot && view === "access" && <AccessView snapshot={snapshot} />}
          {snapshot && view === "audit" && <AuditView snapshot={snapshot} />}
          {snapshot && view === "cluster" && <ClusterView snapshot={snapshot} />}
        </main>
      </div>

      {snapshot && currentArtifact && <ArtifactDrawer key={currentArtifact.id} artifact={currentArtifact} principalId={bootstrap.session.actorId} identityDirectory={snapshot} onClose={() => setSelectedArtifact(undefined)} onOpenRecording={(recordingId) => {
        setSelectedArtifact(undefined);
        navigate("recordings", recordingId);
      }} />}
      {currentTask && <TaskDrawer task={currentTask} onClose={() => setSelectedTask(undefined)} />}
    </div>
      {uploadsOpen && uploadQueue && <UploadPanel queue={uploadQueue} state={uploadState} onClose={() => setUploadsOpen(false)} onView={bootstrap.canReadInstallation ? viewUpload : undefined} />}
    </>
  );
}
