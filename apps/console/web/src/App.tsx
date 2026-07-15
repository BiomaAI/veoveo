import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  Activity,
  Archive,
  Bot,
  Boxes,
  Check,
  Copy,
  Download,
  FileStack,
  Gauge,
  KeyRound,
  Link2,
  LogOut,
  MapPinned,
  Menu,
  Network,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
  UserRound,
  Users,
  X
} from "lucide-react";
import {
  artifactDownloadUrl,
  cancelTask,
  createArtifactShareLink,
  grantArtifact,
  loadMapAcquisitions,
  loadMapActiveReleases,
  loadMapReleases,
  loadMapMobilityProfiles,
  loadMapSources,
  loadSnapshot,
  logoutConsole,
  revokeArtifactGrant,
  revokeArtifactShareLink,
  registerMapSource,
  registerMapMobilityProfile,
  setArtifactReleaseState,
  startMapAcquisition,
  mutateMapRelease,
} from "./api";
import {
  EmptyState,
  Metric,
  ProgressBar,
  RowLink,
  SectionHeader,
  StatusPill,
} from "./components";
import { formatBytes, formatDate } from "./format";
import type {
  ArtifactSummary,
  InstallationSnapshot,
  MapActiveReleaseSummary,
  MapAcquisitionSummary,
  MapMobilityProfileSummary,
  MapReleaseSummary,
  MapSourceSummary,
  TaskSummary,
} from "./types";

const navItems = [
  { id: "overview", label: "Overview", icon: Gauge },
  { id: "work", label: "Work", icon: Activity },
  { id: "artifacts", label: "Artifacts", icon: Archive },
  { id: "agents", label: "Agents", icon: Bot },
  { id: "recordings", label: "Recordings", icon: FileStack },
  { id: "mcp", label: "MCP topology", icon: Network },
  { id: "map", label: "Map data", icon: MapPinned },
  { id: "access", label: "Access", icon: ShieldCheck },
  { id: "audit", label: "Audit", icon: KeyRound },
  { id: "installation", label: "Installation", icon: Settings }
] as const;

type ViewId = (typeof navItems)[number]["id"];

function initialView(): ViewId {
  const value = window.location.hash.replace(/^#\/?/, "");
  return navItems.some((item) => item.id === value) ? (value as ViewId) : "overview";
}

export function App() {
  const [snapshot, setSnapshot] = useState<InstallationSnapshot>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const [view, setView] = useState<ViewId>(initialView);
  const [mobileNav, setMobileNav] = useState(false);
  const [selectedArtifact, setSelectedArtifact] = useState<ArtifactSummary>();
  const [selectedTask, setSelectedTask] = useState<TaskSummary>();
  const [refreshing, setRefreshing] = useState(false);
  const [signingOut, setSigningOut] = useState(false);

  const refresh = async () => {
    setRefreshing(true);
    setError(undefined);
    try {
      setSnapshot(await loadSnapshot());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Console data could not be loaded");
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  };

  useEffect(() => {
    const controller = new AbortController();
    loadSnapshot(controller.signal)
      .then(setSnapshot)
      .catch((cause: unknown) => {
        if (!controller.signal.aborted) {
          setError(cause instanceof Error ? cause.message : "Console data could not be loaded");
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, []);

  const navigate = (next: ViewId) => {
    setView(next);
    setMobileNav(false);
    window.history.replaceState(null, "", `#/${next}`);
  };

  const signOut = async () => {
    setSigningOut(true);
    try {
      await logoutConsole();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Sign out failed");
      setSigningOut(false);
    }
  };

  if (loading) {
    return (
      <div className="center-state">
        <div className="loading-mark" aria-label="Loading" />
      </div>
    );
  }

  if (!snapshot || error) {
    return (
      <div className="center-state error-state">
        <Boxes size={30} />
        <h1>Console unavailable</h1>
        <p>{error ?? "No installation snapshot was returned."}</p>
        <button className="button button-primary" onClick={refresh}>
          <RefreshCw size={15} /> Retry
        </button>
      </div>
    );
  }

  const title = navItems.find((item) => item.id === view)?.label ?? "Overview";
  const currentArtifact = selectedArtifact && snapshot.artifacts.find((item) => item.id === selectedArtifact.id);
  const currentTask = selectedTask && snapshot.tasks.find((item) => item.id === selectedTask.id);

  return (
    <div className="app-shell">
      <aside className={`sidebar ${mobileNav ? "sidebar-open" : ""}`}>
        <div className="brand">
          <div className="brand-mark" aria-hidden="true">V</div>
          <div>
            <strong>Veoveo</strong>
            <span>Operations</span>
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
          <div className={`live-dot ${snapshot.services.some((service) => service.state === "offline") ? "live-off" : ""}`} />
          <div>
            <strong>{snapshot.installation.offlineMode ? "Offline mode" : "Installation online"}</strong>
            <span>SurrealDB {snapshot.installation.databaseTopology}</span>
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
            <label className="tenant-select">
              <Users size={15} />
              <select value={snapshot.session.tenantId} aria-label="Tenant" disabled={snapshot.session.availableTenants.length <= 1}>
                {snapshot.session.availableTenants.map((tenant) => <option key={tenant.id} value={tenant.id}>{tenant.name}</option>)}
              </select>
            </label>
            <button className="icon-button" onClick={refresh} title="Refresh" disabled={refreshing}>
              <RefreshCw size={17} className={refreshing ? "spin" : ""} />
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
          {view === "recordings" && <RecordingsView snapshot={snapshot} />}
          {view === "mcp" && <McpView snapshot={snapshot} />}
          {view === "map" && <MapDataView />}
          {view === "access" && <AccessView snapshot={snapshot} />}
          {view === "audit" && <AuditView snapshot={snapshot} />}
          {view === "installation" && <InstallationView snapshot={snapshot} />}
        </main>
      </div>

      {currentArtifact && <ArtifactDrawer artifact={currentArtifact} onClose={() => setSelectedArtifact(undefined)} onChanged={refresh} />}
      {currentTask && <TaskDrawer task={currentTask} onClose={() => setSelectedTask(undefined)} onChanged={refresh} />}
    </div>
  );
}

function Overview({ snapshot, onTask, onArtifact }: { snapshot: InstallationSnapshot; onTask: (task: TaskSummary) => void; onArtifact: (artifact: ArtifactSummary) => void }) {
  const activeTasks = snapshot.tasks.filter((task) => ["queued", "running", "waiting", "cancel_requested"].includes(task.state));
  const healthy = snapshot.services.filter((service) => service.state === "healthy").length;
  const snapshotTime = new Date(snapshot.installation.generatedAt).getTime();
  const expiring = snapshot.artifacts.filter((artifact) => artifact.retentionExpiresAt && new Date(artifact.retentionExpiresAt).getTime() < snapshotTime + 30 * 86400_000).length;
  return (
    <>
      <div className="metrics-grid">
        <Metric label="Active work" value={String(activeTasks.length)} detail={`${snapshot.tasks.filter((task) => task.state === "waiting").length} waiting`} />
        <Metric label="Services" value={`${healthy}/${snapshot.services.length}`} detail={healthy === snapshot.services.length ? "All healthy" : "Attention required"} />
        <Metric label="Artifacts" value={String(snapshot.artifacts.length)} detail={`${expiring} expire within 30 days`} />
        <Metric label="Agents" value={String(snapshot.agents.length)} detail={`${snapshot.agents.filter((agent) => agent.state === "running").length} active now`} />
      </div>
      <div className="overview-grid">
        <section className="panel panel-wide">
          <SectionHeader title="Active work" count={activeTasks.length} />
          <TaskTable tasks={activeTasks} onSelect={onTask} compact />
        </section>
        <section className="panel">
          <SectionHeader title="Installation health" />
          <div className="health-list">
            {snapshot.services.map((service) => (
              <div key={service.id} className="health-row">
                <div><strong>{service.name}</strong><span>{service.detail}</span></div>
                <div className="health-tail"><StatusPill value={service.state} />{service.latencyMs !== undefined && <span>{service.latencyMs} ms</span>}</div>
              </div>
            ))}
          </div>
        </section>
        <section className="panel panel-wide">
          <SectionHeader title="Recent artifacts" count={snapshot.artifacts.length} />
          <ArtifactTable artifacts={snapshot.artifacts.slice(0, 4)} onSelect={onArtifact} compact />
        </section>
        <section className="panel">
          <SectionHeader title="Recent decisions" />
          <div className="audit-stream">
            {snapshot.audit.slice(0, 4).map((event) => (
              <div key={event.id} className="audit-item">
                <span className={`decision decision-${event.outcome}`}><Check size={12} /></span>
                <div><strong>{event.action}</strong><span>{event.actor} · {event.resource}</span></div>
                <time>{formatDate(event.occurredAt)}</time>
              </div>
            ))}
          </div>
        </section>
      </div>
    </>
  );
}

function Toolbar({ query, setQuery, state, setState, placeholder }: { query: string; setQuery: (value: string) => void; state: string; setState: (value: string) => void; placeholder: string }) {
  return (
    <div className="toolbar">
      <label className="search-control"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={placeholder} /></label>
      <label className="filter-control"><SlidersHorizontal size={15} /><select value={state} onChange={(event) => setState(event.target.value)} aria-label="State filter"><option value="all">All states</option><option value="running">Running</option><option value="waiting">Waiting</option><option value="succeeded">Succeeded</option><option value="failed">Failed</option><option value="private">Private</option><option value="releasable">Releasable</option><option value="released">Released</option></select></label>
    </div>
  );
}

function WorkView({ tasks, onSelect }: { tasks: TaskSummary[]; onSelect: (task: TaskSummary) => void }) {
  const [query, setQuery] = useState("");
  const [state, setState] = useState("all");
  const rows = useMemo(() => tasks.filter((task) => (state === "all" || task.state === state) && `${task.id} ${task.type} ${task.server} ${task.owner}`.toLowerCase().includes(query.toLowerCase())), [tasks, query, state]);
  return <section className="panel full-panel"><SectionHeader title="Tasks" count={rows.length} actions={<Toolbar query={query} setQuery={setQuery} state={state} setState={setState} placeholder="Search tasks" />} /><TaskTable tasks={rows} onSelect={onSelect} /></section>;
}

function TaskTable({ tasks, onSelect, compact = false }: { tasks: TaskSummary[]; onSelect: (task: TaskSummary) => void; compact?: boolean }) {
  if (!tasks.length) return <EmptyState>No tasks match the current view.</EmptyState>;
  return (
    <div className="table-scroll"><table><thead><tr><th>Task</th><th>State</th><th>Progress</th>{!compact && <th>Recovery</th>}<th>Owner</th><th>Updated</th><th aria-label="Open" /></tr></thead><tbody>{tasks.map((task) => <tr key={task.id} onClick={() => onSelect(task)} tabIndex={0}><td><strong>{task.type}</strong><span className="mono subdued">{task.id.slice(0, 13)}… · {task.server}</span></td><td><StatusPill value={task.state} /></td><td><ProgressBar value={task.progress} /></td>{!compact && <td><span className="code-label">{task.recoveryClass}</span></td>}<td>{task.owner}</td><td>{formatDate(task.updatedAt)}</td><td><RowLink /></td></tr>)}</tbody></table></div>
  );
}

function ArtifactsView({ artifacts, onSelect }: { artifacts: ArtifactSummary[]; onSelect: (artifact: ArtifactSummary) => void }) {
  const [query, setQuery] = useState("");
  const [state, setState] = useState("all");
  const rows = useMemo(() => artifacts.filter((artifact) => (state === "all" || artifact.releaseState === state) && `${artifact.id} ${artifact.filename} ${artifact.owner} ${artifact.labels.join(" ")}`.toLowerCase().includes(query.toLowerCase())), [artifacts, query, state]);
  return <section className="panel full-panel"><SectionHeader title="Artifacts" count={rows.length} actions={<Toolbar query={query} setQuery={setQuery} state={state} setState={setState} placeholder="Search artifacts" />} /><ArtifactTable artifacts={rows} onSelect={onSelect} /></section>;
}

function ArtifactTable({ artifacts, onSelect, compact = false }: { artifacts: ArtifactSummary[]; onSelect: (artifact: ArtifactSummary) => void; compact?: boolean }) {
  if (!artifacts.length) return <EmptyState>No artifacts match the current view.</EmptyState>;
  return <div className="table-scroll"><table><thead><tr><th>Artifact</th><th>Release</th><th>Access</th>{!compact && <th>Classification</th>}<th>Size</th><th>Created</th><th aria-label="Open" /></tr></thead><tbody>{artifacts.map((artifact) => <tr key={artifact.id} onClick={() => onSelect(artifact)} tabIndex={0}><td><strong>{artifact.filename}</strong><span className="mono subdued">{artifact.id.slice(0, 13)}…</span></td><td><StatusPill value={artifact.releaseState} /></td><td><span>{artifact.authorizedGrants} grants</span><span className="subdued">{artifact.activeLinks} active links</span></td>{!compact && <td><span className="code-label">{artifact.classification}</span></td>}<td>{formatBytes(artifact.byteLength)}</td><td>{formatDate(artifact.createdAt)}</td><td><RowLink /></td></tr>)}</tbody></table></div>;
}

function AgentsView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return <section className="panel full-panel"><SectionHeader title="Agents" count={snapshot.agents.length} /><div className="item-grid">{snapshot.agents.map((agent) => <article className="item-card" key={agent.id}><div className="item-card-head"><div className="object-icon"><Bot size={18} /></div><StatusPill value={agent.state} /></div><h3>{agent.name}</h3><span className="mono subdued">{agent.id}</span><dl><div><dt>Profile</dt><dd>{agent.profile}</dd></div><div><dt>Pending wakes</dt><dd>{agent.pendingWakes}</dd></div><div><dt>Last episode</dt><dd>{formatDate(agent.lastEpisodeAt)}</dd></div></dl><footer>{agent.detail}</footer></article>)}</div></section>;
}

function RecordingsView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return <section className="panel full-panel"><SectionHeader title="Recordings" count={snapshot.recordings.length} /><div className="table-scroll"><table><thead><tr><th>Recording</th><th>State</th><th>Application</th><th>Segments</th><th>Size</th><th>Started</th><th>Ended</th></tr></thead><tbody>{snapshot.recordings.map((recording) => <tr key={recording.id}><td><strong>{recording.recordingKey}</strong><span className="mono subdued">{recording.id.slice(0, 13)}…</span></td><td><StatusPill value={recording.state} /></td><td>{recording.application}</td><td>{recording.segments}</td><td>{formatBytes(recording.byteLength)}</td><td>{formatDate(recording.startedAt)}</td><td>{formatDate(recording.endedAt)}</td></tr>)}</tbody></table></div></section>;
}

function McpView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return <><section className="panel full-panel"><SectionHeader title="Hosted MCP servers" count={snapshot.servers.length} /><div className="table-scroll"><table><thead><tr><th>Server</th><th>State</th><th>Capabilities</th><th>Profiles</th><th>Transport</th><th>Endpoint</th></tr></thead><tbody>{snapshot.servers.map((server) => <tr key={server.id}><td><strong>{server.name}</strong><span className="mono subdued">{server.id}</span></td><td><StatusPill value={server.state} /></td><td>{server.tools} tools · {server.resources} resources · {server.prompts} prompts</td><td><div className="tags">{server.profiles.map((profile) => <span key={profile}>{profile}</span>)}</div></td><td><span className="code-label">{server.transport}</span></td><td className="mono">{server.endpoint}</td></tr>)}</tbody></table></div></section></>;
}

function MapDataView() {
  const [sources, setSources] = useState<MapSourceSummary[]>([]);
  const [acquisitions, setAcquisitions] = useState<MapAcquisitionSummary[]>([]);
  const [releases, setReleases] = useState<MapReleaseSummary[]>([]);
  const [mobilityProfiles, setMobilityProfiles] = useState<MapMobilityProfileSummary[]>([]);
  const [activeReleases, setActiveReleases] = useState<MapActiveReleaseSummary[]>([]);
  const [selectedSource, setSelectedSource] = useState("");
  const [sourceJson, setSourceJson] = useState("");
  const [mobilityProfileJson, setMobilityProfileJson] = useState("");
  const [coverage, setCoverage] = useState({ west: -180, south: -90, east: 180, north: 90 });
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string>();

  const refresh = async () => {
    setError(undefined);
    try {
      const [nextSources, nextAcquisitions, nextReleases, nextMobilityProfiles, nextActiveReleases] = await Promise.all([
        loadMapSources(),
        loadMapAcquisitions(),
        loadMapReleases(),
        loadMapMobilityProfiles(),
        loadMapActiveReleases(),
      ]);
      setSources(nextSources);
      setAcquisitions(nextAcquisitions);
      setReleases(nextReleases);
      setMobilityProfiles(nextMobilityProfiles);
      setActiveReleases(nextActiveReleases);
      if (!selectedSource && nextSources[0]) setSelectedSource(nextSources[0].source_id);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Map administration could not be loaded");
    }
  };

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      loadMapSources(),
      loadMapAcquisitions(),
      loadMapReleases(),
      loadMapMobilityProfiles(),
      loadMapActiveReleases(),
    ])
      .then(([nextSources, nextAcquisitions, nextReleases, nextMobilityProfiles, nextActiveReleases]) => {
        if (cancelled) return;
        setSources(nextSources);
        setAcquisitions(nextAcquisitions);
        setReleases(nextReleases);
        setMobilityProfiles(nextMobilityProfiles);
        setActiveReleases(nextActiveReleases);
        if (nextSources[0]) setSelectedSource(nextSources[0].source_id);
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : "Map administration could not be loaded");
        }
      });
    return () => { cancelled = true; };
  }, []);

  const run = async (operation: () => Promise<unknown>) => {
    setPending(true);
    setError(undefined);
    try {
      await operation();
      await refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Map administration failed");
    } finally {
      setPending(false);
    }
  };

  const submitSource = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => registerMapSource(JSON.parse(sourceJson) as unknown));
  };

  const submitAcquisition = (event: FormEvent) => {
    event.preventDefault();
    void run(() => startMapAcquisition(selectedSource, coverage));
  };

  const submitMobilityProfile = (event: FormEvent) => {
    event.preventDefault();
    void run(async () => registerMapMobilityProfile(JSON.parse(mobilityProfileJson) as unknown));
  };

  const releaseAction = (release: MapReleaseSummary, action: "activate" | "rollback" | "quarantine") => {
    const pointerVersion = activeReleases.find((pointer) => pointer.dataset_id === release.dataset_id)?.record_version ?? 0;
    const warning = action === "quarantine"
      ? `Quarantine ${release.release_id}?`
      : `${action} ${release.release_id} using active pointer version ${pointerVersion}?`;
    if (window.confirm(warning)) void run(() => mutateMapRelease(release, action, pointerVersion));
  };

  return <div className="map-admin-layout">
    {error && <div className="action-error">{error}</div>}
    <section className="panel full-panel">
      <SectionHeader title="Authoritative sources" count={sources.length} actions={<button className="button button-secondary" onClick={() => void refresh()} disabled={pending}><RefreshCw size={14} /> Refresh</button>} />
      <div className="table-scroll"><table><thead><tr><th>Source</th><th>Adapter</th><th>Authority</th><th>Families</th><th>State</th><th>Version</th></tr></thead><tbody>{sources.map((source) => <tr key={source.source_id}><td><strong>{source.name}</strong><span className="mono subdued">{source.source_id}</span></td><td><span className="code-label">{source.adapter_kind}</span></td><td>{source.authority}</td><td>{source.map_families.join(", ")}</td><td><StatusPill value={source.enabled ? "active" : "disabled"} /></td><td>r{source.record_version}</td></tr>)}</tbody></table></div>
      {!sources.length && <EmptyState>No governed map sources are registered.</EmptyState>}
    </section>
    <section className="panel map-admin-form">
      <SectionHeader title="Register source" />
      <form onSubmit={submitSource}><label>Canonical RegisteredSource JSON<textarea value={sourceJson} onChange={(event) => setSourceJson(event.target.value)} rows={9} spellCheck={false} required /></label><button className="button button-primary" disabled={pending || !sourceJson.trim()}>Register source</button></form>
    </section>
    <section className="panel map-admin-form">
      <SectionHeader title="Acquire release" />
      <form onSubmit={submitAcquisition}><label>Source<select value={selectedSource} onChange={(event) => setSelectedSource(event.target.value)} required>{sources.map((source) => <option key={source.source_id} value={source.source_id}>{source.name}</option>)}</select></label><div className="coverage-grid">{(["west", "south", "east", "north"] as const).map((field) => <label key={field}>{field}<input type="number" value={coverage[field]} min={field === "south" || field === "north" ? -90 : -180} max={field === "south" || field === "north" ? 90 : 180} onChange={(event) => setCoverage({ ...coverage, [field]: Number(event.target.value) })} /></label>)}</div><button className="button button-primary" disabled={pending || !selectedSource}>Start acquisition</button></form>
    </section>
    <section className="panel full-panel">
      <SectionHeader title="Acquisition jobs" count={acquisitions.length} />
      <div className="table-scroll"><table><thead><tr><th>Acquisition</th><th>Source</th><th>Status</th><th>Phase</th><th>Message</th><th>Updated</th></tr></thead><tbody>{acquisitions.map((job) => <tr key={job.acquisition_id}><td className="mono">{job.acquisition_id}</td><td className="mono">{job.source_id}</td><td><StatusPill value={job.status} /></td><td>{job.progress.phase}</td><td>{job.progress.message}</td><td>{formatDate(job.updated_at)}</td></tr>)}</tbody></table></div>
    </section>
    <section className="panel full-panel">
      <SectionHeader title="Mobility profiles" count={mobilityProfiles.length} />
      <div className="table-scroll"><table><thead><tr><th>Profile</th><th>Family</th><th>Version</th><th>Valid from</th></tr></thead><tbody>{mobilityProfiles.map((profile) => <tr key={`${profile.profile.metadata.profile_id}:${profile.profile.metadata.version}`}><td><strong>{profile.profile.metadata.name}</strong><span className="mono subdued">{profile.profile.metadata.profile_id}</span></td><td><span className="code-label">{profile.family}</span></td><td>v{profile.profile.metadata.version}</td><td>{formatDate(profile.profile.metadata.valid_from)}</td></tr>)}</tbody></table></div>
      {!mobilityProfiles.length && <EmptyState>No human or vehicle mobility profiles are registered.</EmptyState>}
    </section>
    <section className="panel full-panel map-admin-form">
      <SectionHeader title="Register mobility profile" />
      <form onSubmit={submitMobilityProfile}><label>Canonical MobilityProfile JSON<textarea value={mobilityProfileJson} onChange={(event) => setMobilityProfileJson(event.target.value)} rows={9} spellCheck={false} required /></label><button className="button button-primary" disabled={pending || !mobilityProfileJson.trim()}>Register profile version</button></form>
    </section>
    <section className="panel full-panel">
      <SectionHeader title="Dataset releases" count={releases.length} />
      <div className="table-scroll"><table><thead><tr><th>Release</th><th>Dataset</th><th>Version</th><th>State</th><th>Record</th><th>Actions</th></tr></thead><tbody>{releases.map((release) => <tr key={release.release_id}><td className="mono">{release.release_id}</td><td className="mono">{release.dataset_id}</td><td>{release.version_label}</td><td><StatusPill value={release.state} /></td><td>r{release.record_version}</td><td><div className="row-actions">{release.state === "staged" && <button className="button button-primary" disabled={pending} onClick={() => releaseAction(release, "activate")}>Activate</button>}{release.state === "active" && <button className="button button-secondary" disabled={pending} onClick={() => releaseAction(release, "activate")}>Reconcile</button>}<button className="button button-secondary" disabled={pending || release.state === "quarantined"} onClick={() => releaseAction(release, "rollback")}>Rollback</button><button className="button button-secondary" disabled={pending || release.state === "quarantined"} onClick={() => releaseAction(release, "quarantine")}>Quarantine</button></div></td></tr>)}</tbody></table></div>
    </section>
  </div>;
}

function AccessView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return <section className="panel full-panel"><SectionHeader title="Policy revisions" count={snapshot.policies.length} actions={<button className="button button-secondary"><ShieldCheck size={15} /> New revision</button>} /><div className="table-scroll"><table><thead><tr><th>Policy</th><th>State</th><th>Revision</th><th>Rules</th><th>Updated</th><th aria-label="Open" /></tr></thead><tbody>{snapshot.policies.map((policy) => <tr key={policy.id}><td><strong>{policy.name}</strong><span className="mono subdued">{policy.id}</span></td><td><StatusPill value={policy.state} /></td><td>r{policy.revision}</td><td>{policy.rules}</td><td>{formatDate(policy.updatedAt)}</td><td><RowLink /></td></tr>)}</tbody></table></div></section>;
}

function AuditView({ snapshot }: { snapshot: InstallationSnapshot }) {
  const [query, setQuery] = useState("");
  const rows = snapshot.audit.filter((event) => `${event.actor} ${event.action} ${event.resource} ${event.outcome}`.toLowerCase().includes(query.toLowerCase()));
  return <section className="panel full-panel"><SectionHeader title="Audit events" count={rows.length} actions={<><label className="search-control"><Search size={15} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search audit" /></label><button className="button button-secondary"><Download size={15} /> Export</button></>} /><div className="table-scroll"><table><thead><tr><th>Time</th><th>Outcome</th><th>Actor</th><th>Action</th><th>Resource</th><th>Source</th><th>Trace</th></tr></thead><tbody>{rows.map((event) => <tr key={event.id}><td>{formatDate(event.occurredAt)}</td><td><StatusPill value={event.outcome} /></td><td>{event.actor}</td><td className="mono">{event.action}</td><td>{event.resource}</td><td className="mono">{event.sourceIp ?? "-"}</td><td className="mono subdued">{event.traceId ?? "-"}</td></tr>)}</tbody></table></div></section>;
}

function InstallationView({ snapshot }: { snapshot: InstallationSnapshot }) {
  return <div className="settings-layout"><section className="panel"><SectionHeader title="Runtime" /><dl className="definition-list"><div><dt>Version</dt><dd>{snapshot.installation.version}</dd></div><div><dt>Mode</dt><dd>{snapshot.installation.offlineMode ? "Air-gapped" : "Connected"}</dd></div><div><dt>Database</dt><dd>SurrealDB 3.2 · RocksDB</dd></div><div><dt>Topology</dt><dd>{snapshot.installation.databaseTopology}</dd></div><div><dt>Tenant</dt><dd>{snapshot.session.tenantName}</dd></div></dl></section><section className="panel"><SectionHeader title="Service endpoints" /><div className="health-list">{snapshot.services.map((service) => <div key={service.id} className="health-row"><div><strong>{service.name}</strong><span>{service.kind}</span></div><StatusPill value={service.state} /></div>)}</div></section></div>;
}

function DrawerShell({ title, subtitle, onClose, children }: { title: string; subtitle: string; onClose: () => void; children: React.ReactNode }) {
  return <div className="drawer-layer"><button className="drawer-scrim" aria-label="Close details" onClick={onClose} /><aside className="drawer" aria-label={`${title} details`}><header><div><span>{subtitle}</span><h2>{title}</h2></div><button className="icon-button" onClick={onClose} title="Close"><X size={18} /></button></header>{children}</aside></div>;
}

function ArtifactDrawer({ artifact, onClose, onChanged }: { artifact: ArtifactSummary; onClose: () => void; onChanged: () => Promise<void> }) {
  const [copied, setCopied] = useState(false);
  const [linkCopied, setLinkCopied] = useState(false);
  const [newLink, setNewLink] = useState<string>();
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const [subjectKind, setSubjectKind] = useState<"user" | "group">("user");
  const [subjectId, setSubjectId] = useState("");
  const [grantLevel, setGrantLevel] = useState<"read" | "write" | "admin">("read");
  const [linkDays, setLinkDays] = useState(7);
  const [maxDownloads, setMaxDownloads] = useState("");
  const copyId = async () => { await navigator.clipboard.writeText(`artifact://${artifact.id}`); setCopied(true); window.setTimeout(() => setCopied(false), 1500); };
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    setActionError(undefined);
    try {
      await action();
      await onChanged();
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Artifact operation failed");
    } finally {
      setPending(false);
    }
  };
  const submitGrant = (event: FormEvent) => {
    event.preventDefault();
    if (!subjectId.trim()) return;
    void run(async () => {
      await grantArtifact(artifact.id, { kind: subjectKind, id: subjectId.trim() }, grantLevel);
      setSubjectId("");
    });
  };
  const submitLink = async (event: FormEvent) => {
    event.preventDefault();
    setPending(true);
    setActionError(undefined);
    try {
      const max = maxDownloads ? Number.parseInt(maxDownloads, 10) : undefined;
      const created = await createArtifactShareLink(
        artifact.id,
        new Date(Date.now() + linkDays * 86_400_000).toISOString(),
        max && max > 0 ? max : undefined
      );
      setNewLink(created.url);
      await onChanged();
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Share link creation failed");
    } finally {
      setPending(false);
    }
  };
  const copyLink = async () => {
    if (!newLink) return;
    await navigator.clipboard.writeText(newLink);
    setLinkCopied(true);
    window.setTimeout(() => setLinkCopied(false), 1500);
  };
  return <DrawerShell title={artifact.filename} subtitle="Artifact" onClose={onClose}>
    <div className="drawer-body">
      <div className="drawer-status"><StatusPill value={artifact.releaseState} /><span>{formatBytes(artifact.byteLength)}</span><span>{artifact.mediaType}</span></div>
      {actionError && <div className="action-error">{actionError}</div>}
      <section>
        <h3>Identity</h3>
        <button className="copy-field" onClick={() => void copyId()}><span className="mono">artifact://{artifact.id}</span>{copied ? <Check size={15} /> : <Copy size={15} />}</button>
        <dl className="definition-list compact"><div><dt>Owner</dt><dd>{artifact.owner}</dd></div><div><dt>Created</dt><dd>{formatDate(artifact.createdAt)}</dd></div><div><dt>Retention</dt><dd>{formatDate(artifact.retentionExpiresAt)}</dd></div></dl>
      </section>
      <section>
        <h3>Release state</h3>
        <div className="segmented" role="group" aria-label="Artifact release state">
          {(["private", "releasable", "released"] as const).map((state) => <button key={state} className={artifact.releaseState === state ? "segment-active" : ""} disabled={pending || artifact.releaseState === state} onClick={() => void run(() => setArtifactReleaseState(artifact.id, state))}>{state}</button>)}
        </div>
      </section>
      <section>
        <div className="drawer-section-head"><h3>Authorized access</h3><span className="subdued">{artifact.authorizedGrants} grants</span></div>
        <form className="inline-form" onSubmit={submitGrant}>
          <select value={subjectKind} onChange={(event) => setSubjectKind(event.target.value as "user" | "group")} aria-label="Grant subject type"><option value="user">User</option><option value="group">Group</option></select>
          <input value={subjectId} onChange={(event) => setSubjectId(event.target.value)} placeholder="Principal or group ID" aria-label="Grant subject ID" required />
          <select value={grantLevel} onChange={(event) => setGrantLevel(event.target.value as "read" | "write" | "admin")} aria-label="Grant permission"><option value="read">Read</option><option value="write">Write</option><option value="admin">Admin</option></select>
          <button className="icon-button" type="submit" title="Add grant" disabled={pending || !subjectId.trim()}><UserRound size={15} /></button>
        </form>
        <div className="access-list">
          {artifact.grants.map((grant) => <div className="access-row" key={`${grant.subjectKind}:${grant.subject}`}><div><strong>{grant.subject}</strong><span>{grant.subjectKind} · {grant.permission}</span></div><button className="icon-button icon-danger" title="Revoke grant" disabled={pending} onClick={() => void run(() => revokeArtifactGrant(artifact.id, { kind: grant.subjectKind, id: grant.subject }))}><Trash2 size={14} /></button></div>)}
          {!artifact.grants.length && <div className="access-empty">No projected grants</div>}
        </div>
      </section>
      <section>
        <div className="drawer-section-head"><h3>Anyone with link</h3><span className="subdued">{artifact.activeLinks} active</span></div>
        {newLink && <div className="one-time-link"><span>New link · shown once</span><button className="copy-field" onClick={() => void copyLink()}><span className="mono">{newLink}</span>{linkCopied ? <Check size={15} /> : <Copy size={15} />}</button></div>}
        <form className="inline-form share-form" onSubmit={(event) => void submitLink(event)}>
          <label><span>Days</span><input type="number" min="1" max="30" value={linkDays} onChange={(event) => setLinkDays(Number(event.target.value))} /></label>
          <label><span>Download limit</span><input type="number" min="1" value={maxDownloads} onChange={(event) => setMaxDownloads(event.target.value)} placeholder="Unlimited" /></label>
          <button className="button button-secondary" type="submit" disabled={pending || artifact.releaseState === "private"}><Link2 size={14} /> Create link</button>
        </form>
        <div className="access-list">
          {artifact.shareLinks.map((link) => <div className="access-row" key={link.id}><div><strong>{link.active ? "Active bearer link" : "Inactive bearer link"}</strong><span>{formatDate(link.expiresAt)} · {link.downloadCount}{link.maxDownloads ? ` / ${link.maxDownloads}` : ""} downloads</span></div>{link.active && <button className="icon-button icon-danger" title="Revoke link" disabled={pending} onClick={() => void run(() => revokeArtifactShareLink(artifact.id, link.id))}><Trash2 size={14} /></button>}</div>)}
          {!artifact.shareLinks.length && <div className="access-empty">No share links</div>}
        </div>
      </section>
    </div>
    <footer className="drawer-footer"><a className="button button-secondary" href={artifactDownloadUrl(artifact.id)}><Download size={15} /> Download</a></footer>
  </DrawerShell>;
}

function TaskDrawer({ task, onClose, onChanged }: { task: TaskSummary; onClose: () => void; onChanged: () => Promise<void> }) {
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState<string>();
  const cancellable = ["queued", "running", "waiting", "cancel_requested"].includes(task.state);
  const cancel = async () => {
    setPending(true);
    setActionError(undefined);
    try {
      await cancelTask(task.id);
      await onChanged();
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Task cancellation failed");
    } finally {
      setPending(false);
    }
  };
  return <DrawerShell title={task.type} subtitle="Task" onClose={onClose}><div className="drawer-body"><div className="drawer-status"><StatusPill value={task.state} /><span>{task.server}</span><span>{task.owner}</span></div>{actionError && <div className="action-error">{actionError}</div>}<section><h3>Progress</h3><ProgressBar value={task.progress} /><p className="detail-copy">{task.message ?? "No status message"}</p></section><section><h3>Execution</h3><dl className="definition-list compact"><div><dt>Task ID</dt><dd className="mono hash">{task.id}</dd></div><div><dt>Recovery</dt><dd><span className="code-label">{task.recoveryClass}</span></dd></div><div><dt>Created</dt><dd>{formatDate(task.createdAt)}</dd></div><div><dt>Updated</dt><dd>{formatDate(task.updatedAt)}</dd></div><div><dt>Result artifact</dt><dd className="mono">{task.resultArtifactId ?? "-"}</dd></div></dl></section></div><footer className="drawer-footer"><button className="button button-secondary" disabled={!cancellable || pending} onClick={() => void cancel()}>Cancel task</button></footer></DrawerShell>;
}
