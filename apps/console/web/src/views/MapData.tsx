import { useState, type FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import { EmptyState, SectionHeader, StatusPill } from "../components/primitives";
import { useConfirm } from "../components/confirm";
import { formatDate } from "../format";
import {
  queryKeys,
  useMapAdministration,
  useMutateMapRelease,
  useRegisterMapMobilityProfile,
  useRegisterMapSource,
  useStartMapAcquisition,
} from "../queries";
import type { MapReleaseSummary } from "../types";

export function MapDataView() {
  const queryClient = useQueryClient();
  const confirm = useConfirm();
  const administration = useMapAdministration();
  const registerSource = useRegisterMapSource();
  const registerMobilityProfile = useRegisterMapMobilityProfile();
  const startAcquisition = useStartMapAcquisition();
  const mutateRelease = useMutateMapRelease();

  const [selectedSource, setSelectedSource] = useState("");
  const [sourceJson, setSourceJson] = useState("");
  const [mobilityProfileJson, setMobilityProfileJson] = useState("");
  const [coverage, setCoverage] = useState({ west: -90.13, south: 13.15, east: -87.68, north: 14.45 });
  const [actionError, setActionError] = useState<string>();

  const sources = administration.data?.sources ?? [];
  const acquisitions = administration.data?.acquisitions ?? [];
  const releases = administration.data?.releases ?? [];
  const mobilityProfiles = administration.data?.mobilityProfiles ?? [];
  const activeReleases = administration.data?.activeReleases ?? [];
  const currentSource = selectedSource || (sources[0]?.source_id ?? "");
  const pending =
    registerSource.isPending || registerMobilityProfile.isPending || startAcquisition.isPending || mutateRelease.isPending;
  const error =
    actionError ??
    (administration.error instanceof Error ? administration.error.message : undefined);

  const refresh = () => void queryClient.invalidateQueries({ queryKey: queryKeys.mapAdmin });

  const run = async (operation: () => Promise<unknown>) => {
    setActionError(undefined);
    try {
      await operation();
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Map administration failed");
    }
  };

  const submitSource = (event: FormEvent) => {
    event.preventDefault();
    void run(() => registerSource.mutateAsync(JSON.parse(sourceJson) as unknown));
  };

  const submitAcquisition = (event: FormEvent) => {
    event.preventDefault();
    void run(() => startAcquisition.mutateAsync({ sourceId: currentSource, coverage }));
  };

  const submitMobilityProfile = (event: FormEvent) => {
    event.preventDefault();
    void run(() => registerMobilityProfile.mutateAsync(JSON.parse(mobilityProfileJson) as unknown));
  };

  const releaseAction = async (release: MapReleaseSummary, action: "activate" | "rollback" | "quarantine") => {
    const pointerVersion = activeReleases.find((pointer) => pointer.dataset_id === release.dataset_id)?.record_version ?? 0;
    const confirmed = await confirm(
      action === "quarantine"
        ? {
            title: "Quarantine release",
            body: <>Quarantine <code>{release.release_id}</code>? Quarantined releases cannot be activated or rolled back.</>,
            confirmLabel: "Quarantine",
            tone: "danger",
          }
        : {
            title: release.state === "active" && action === "activate" ? "Reconcile release" : action === "activate" ? "Activate release" : "Roll back release",
            body: <><code>{release.release_id}</code> will be {action === "rollback" ? "rolled back" : action === "activate" && release.state === "active" ? "reconciled" : "activated"} using active pointer version {pointerVersion}.</>,
            confirmLabel: action === "rollback" ? "Rollback" : release.state === "active" && action === "activate" ? "Reconcile" : "Activate",
          }
    );
    if (confirmed) {
      await run(() => mutateRelease.mutateAsync({ release, action, activePointerVersion: pointerVersion }));
    }
  };

  return <div className="map-admin-layout">
    {error && <div className="action-error">{error}</div>}
    <section className="panel full-panel">
      <SectionHeader title="Authoritative sources" count={sources.length} actions={<button className="button button-secondary" onClick={refresh} disabled={pending || administration.isFetching}><RefreshCw size={14} className={administration.isFetching ? "spin" : ""} /> Refresh</button>} />
      <div className="table-scroll"><table><thead><tr><th>Source</th><th>Adapter</th><th>Authority</th><th>Families</th><th>State</th><th>Version</th></tr></thead><tbody>{sources.map((source) => <tr key={source.source_id}><td><strong>{source.name}</strong><span className="mono subdued">{source.source_id}</span></td><td><span className="code-label">{source.adapter_kind}</span></td><td>{source.authority}</td><td>{source.map_families.join(", ")}</td><td><StatusPill value={source.enabled ? "active" : "disabled"} /></td><td>r{source.record_version}</td></tr>)}</tbody></table></div>
      {!sources.length && <EmptyState>No governed map sources are registered.</EmptyState>}
    </section>
    <section className="panel map-admin-form">
      <SectionHeader title="Register source" />
      <form onSubmit={submitSource}><label>Canonical RegisteredSource JSON<textarea value={sourceJson} onChange={(event) => setSourceJson(event.target.value)} rows={9} spellCheck={false} required /></label><button className="button button-primary" disabled={pending || !sourceJson.trim()}>Register source</button></form>
    </section>
    <section className="panel map-admin-form">
      <SectionHeader title="Acquire release" />
      <form onSubmit={submitAcquisition}><label>Source<select value={currentSource} onChange={(event) => setSelectedSource(event.target.value)} required>{sources.map((source) => <option key={source.source_id} value={source.source_id}>{source.name}</option>)}</select></label><div className="coverage-grid">{(["west", "south", "east", "north"] as const).map((field) => <label key={field}>{field}<input type="number" value={coverage[field]} min={field === "south" || field === "north" ? -90 : -180} max={field === "south" || field === "north" ? 90 : 180} onChange={(event) => setCoverage({ ...coverage, [field]: Number(event.target.value) })} /></label>)}</div><button className="button button-primary" disabled={pending || !currentSource}>Start acquisition</button></form>
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
      <div className="table-scroll"><table><thead><tr><th>Release</th><th>Dataset</th><th>Version</th><th>State</th><th>Record</th><th>Actions</th></tr></thead><tbody>{releases.map((release) => <tr key={release.release_id}><td className="mono">{release.release_id}</td><td className="mono">{release.dataset_id}</td><td>{release.version_label}</td><td><StatusPill value={release.state} /></td><td>r{release.record_version}</td><td><div className="row-actions">{release.state === "staged" && <button className="button button-primary" disabled={pending} onClick={() => void releaseAction(release, "activate")}>Activate</button>}{release.state === "active" && <button className="button button-secondary" disabled={pending} onClick={() => void releaseAction(release, "activate")}>Reconcile</button>}<button className="button button-secondary" disabled={pending || release.state === "quarantined"} onClick={() => void releaseAction(release, "rollback")}>Rollback</button><button className="button button-secondary" disabled={pending || release.state === "quarantined"} onClick={() => void releaseAction(release, "quarantine")}>Quarantine</button></div></td></tr>)}</tbody></table></div>
    </section>
  </div>;
}
