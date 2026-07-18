import { useEffect, useState } from "react";
import {
  Activity,
  Archive,
  Bot,
  Boxes,
  FileStack,
  Gauge,
  KeyRound,
  LayoutGrid,
  LogOut,
  MapPinned,
  Menu,
  Network,
  Palette,
  RefreshCw,
  ShieldCheck,
  Users,
  X
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { logoutConsole } from "./api";
import { useConsoleLiveStream } from "./live";
import { queryKeys, useSnapshot } from "./queries";
import { Overview } from "./views/Overview";
import { WorkView } from "./views/Work";
import { ArtifactsView } from "./views/Artifacts";
import { AgentsView } from "./views/Agents";
import { RecordingsView } from "./views/Recordings";
import { McpView } from "./views/Mcp";
import { AppsView } from "./views/Apps";
import { MapDataView } from "./views/MapData";
import { AccessView } from "./views/Access";
import { AuditView } from "./views/Audit";
import { ClusterView } from "./views/Cluster";
import { ArtifactDrawer } from "./drawers/ArtifactDrawer";
import { TaskDrawer } from "./drawers/TaskDrawer";
import type { ArtifactSummary, TaskSummary } from "./types";
import { consoleThemes, useTheme, type ConsoleTheme } from "./theme";

const navItems = [
  { id: "overview", label: "Overview", icon: Gauge },
  { id: "work", label: "Work", icon: Activity },
  { id: "artifacts", label: "Artifacts", icon: Archive },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "recordings", label: "Recordings", icon: FileStack },
  { id: "mcp", label: "MCP", icon: Network },
  { id: "apps", label: "Apps", icon: LayoutGrid },
  { id: "map", label: "Map data", icon: MapPinned },
  { id: "access", label: "Access", icon: ShieldCheck },
  { id: "audit", label: "Audit", icon: KeyRound },
  { id: "cluster", label: "Cluster", icon: Boxes }
] as const;

type ViewId = (typeof navItems)[number]["id"];

function initialRoute(): { view: ViewId; recordingId?: string } {
  const [value, recordingId] = window.location.hash.replace(/^#\/?/, "").split("/");
  const view = navItems.some((item) => item.id === value) ? (value as ViewId) : "overview";
  return {
    view,
    recordingId: view === "recordings" && recordingId ? recordingId : undefined,
  };
}

function logoSource(logo: string): string {
  return logo.startsWith("data:") ? logo : `data:image/svg+xml;utf8,${encodeURIComponent(logo)}`;
}

export function App() {
  const initial = initialRoute();
  const { theme, setTheme } = useTheme();
  const queryClient = useQueryClient();
  const { data: snapshot, error, isLoading, isFetching } = useSnapshot();
  const liveStatus = useConsoleLiveStream(snapshot?.stream.cursor);
  const [view, setView] = useState<ViewId>(initial.view);
  const [mobileNav, setMobileNav] = useState(false);
  const [selectedArtifact, setSelectedArtifact] = useState<ArtifactSummary>();
  const [selectedTask, setSelectedTask] = useState<TaskSummary>();
  const [selectedRecordingId, setSelectedRecordingId] = useState<string | undefined>(initial.recordingId);
  const [signOutError, setSignOutError] = useState<string>();
  const [signingOut, setSigningOut] = useState(false);

  const refresh = () => void queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });

  const installation = snapshot?.installation;
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

  const signOut = async () => {
    setSigningOut(true);
    try {
      await logoutConsole();
    } catch (cause) {
      setSignOutError(cause instanceof Error ? cause.message : "Sign out failed");
      setSigningOut(false);
    }
  };

  if (isLoading) {
    return (
      <div className="center-state">
        <div className="loading-mark" aria-label="Loading" />
      </div>
    );
  }

  if (!snapshot) {
    const message =
      signOutError ??
      (error instanceof Error ? error.message : "No installation snapshot was returned.");
    return (
      <div className="center-state error-state">
        <Boxes size={30} />
        <h1>Console unavailable</h1>
        <p>{message}</p>
        <div className="error-actions">
          <button className="button button-primary" onClick={refresh}>
            <RefreshCw size={15} /> Retry
          </button>
          <button className="button button-secondary" onClick={() => void signOut()} disabled={signingOut}>
            <LogOut size={15} /> Sign out and authenticate again
          </button>
        </div>
      </div>
    );
  }

  const navigate = (next: ViewId, recordingId?: string) => {
    setView(next);
    setSelectedRecordingId(recordingId);
    setMobileNav(false);
    window.history.replaceState(
      null,
      "",
      recordingId ? `#/${next}/${encodeURIComponent(recordingId)}` : `#/${next}`
    );
  };

  const title = navItems.find((item) => item.id === view)?.label ?? "Overview";
  const currentArtifact = selectedArtifact && snapshot.artifacts.find((item) => item.id === selectedArtifact.id);
  const currentTask = selectedTask && snapshot.tasks.find((item) => item.id === selectedTask.id);

  return (
    <div className="app-shell">
      <aside className={`sidebar ${mobileNav ? "sidebar-open" : ""}`}>
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">
            {snapshot.installation.logo
              ? <img src={logoSource(snapshot.installation.logo)} alt="" />
              : snapshot.installation.name.charAt(0).toUpperCase()}
          </div>
          <div>
            <strong>{snapshot.installation.name}</strong>
            <span>{snapshot.installation.productLabel}</span>
          </div>
          <button className="icon-button mobile-close" onClick={() => setMobileNav(false)} title="Close navigation">
            <X size={18} />
          </button>
        </div>
        <nav aria-label="Primary navigation">
          {navItems.map(({ id, label, icon: Icon }) => (
            <button key={id} className={view === id ? "nav-active" : ""} onClick={() => navigate(id)}>
              <Icon size={17} />
              <span>{label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <div className={`live-dot ${liveStatus === "reconnecting" || snapshot.services.some((service) => service.state === "offline") ? "live-off" : ""}`} />
          <div>
            <strong>{liveStatus === "live" ? "Live" : liveStatus === "reconnecting" ? "Reconnecting" : "Status"}</strong>
            <span>{snapshot.services.filter((service) => service.state === "healthy").length}/{snapshot.services.length} platform services healthy</span>
          </div>
        </div>
      </aside>

      {mobileNav && <button className="nav-scrim" aria-label="Close navigation" onClick={() => setMobileNav(false)} />}

      <div className="main-shell">
        <header className="topbar">
          <div className="topbar-title">
            <button className="icon-button mobile-menu" onClick={() => setMobileNav(true)} title="Open navigation">
              <Menu size={19} />
            </button>
            <div>
              <span>{snapshot.installation.name}</span>
              <h1>{title}</h1>
            </div>
          </div>
          <div className="topbar-actions">
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
              <select value={snapshot.session.tenantId} aria-label="Tenant" disabled={snapshot.session.availableTenants.length <= 1}>
                {snapshot.session.availableTenants.map((tenant) => <option key={tenant.id} value={tenant.id}>{tenant.name}</option>)}
              </select>
            </label>
            <button className="icon-button" onClick={refresh} title="Refresh" disabled={isFetching}>
              <RefreshCw size={17} className={isFetching ? "spin" : ""} />
            </button>
            <div className="user-menu" title={snapshot.session.principalId}>
              <span>{snapshot.session.displayName.split(/\s+/).map((part) => part[0]).join("").slice(0, 2)}</span>
              <strong>{snapshot.session.displayName}</strong>
            </div>
            <button className="icon-button" onClick={() => void signOut()} title="Sign out" disabled={signingOut}>
              <LogOut size={17} />
            </button>
          </div>
        </header>

        <main className="content">
          {view === "overview" && <Overview snapshot={snapshot} onArtifact={setSelectedArtifact} onTask={setSelectedTask} />}
          {view === "work" && <WorkView tasks={snapshot.tasks} onSelect={setSelectedTask} />}
          {view === "artifacts" && <ArtifactsView artifacts={snapshot.artifacts} onSelect={setSelectedArtifact} />}
          {view === "agents" && <AgentsView snapshot={snapshot} />}
          {view === "recordings" && <RecordingsView snapshot={snapshot} initialRecordingId={selectedRecordingId} onRecordingSelect={(recordingId) => {
            setSelectedRecordingId(recordingId);
            window.history.replaceState(null, "", `#/recordings/${encodeURIComponent(recordingId)}`);
          }} />}
          {view === "mcp" && <McpView snapshot={snapshot} />}
          {view === "apps" && <AppsView />}
          {view === "map" && <MapDataView />}
          {view === "access" && <AccessView snapshot={snapshot} />}
          {view === "audit" && <AuditView snapshot={snapshot} />}
          {view === "cluster" && <ClusterView snapshot={snapshot} />}
        </main>
      </div>

      {currentArtifact && <ArtifactDrawer key={currentArtifact.id} artifact={currentArtifact} principalId={snapshot.session.principalId} onClose={() => setSelectedArtifact(undefined)} onOpenRecording={(recordingId) => {
        setSelectedArtifact(undefined);
        navigate("recordings", recordingId);
      }} />}
      {currentTask && <TaskDrawer task={currentTask} onClose={() => setSelectedTask(undefined)} />}
    </div>
  );
}
