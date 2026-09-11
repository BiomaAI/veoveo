import { lazy, Suspense, useEffect, useState, useSyncExternalStore } from "react";
import { z } from "zod";
import { Monitor, Play, Plus, RefreshCw, Square } from "lucide-react";
import { ComputersController } from "./controller";
import { AccessPanel } from "./AccessPanel";
import { AutomationPanel } from "./AutomationPanel";
import { CliConnect } from "./CliConnect";
import { lifecycle, readComputers, readOperation } from "./api";
import { watchComputers } from "./events";
import type { CapacityAvailability, ComputerPhase } from "../generated/computers";
import "./computers.css";
const TerminalPanel = lazy(() =>
  import("./TerminalPanel").then((module) => ({ default: module.TerminalPanel })),
);

const availability: Record<CapacityAvailability, string> = {
  setup_required: "Computer capacity has not been configured for this installation.",
  available: "Capacity is available.",
  exhausted: "Computer capacity is full.",
  compute_unavailable: "Compute is temporarily unavailable. Existing work may still be running.",
  storage_unavailable: "Retained storage is temporarily unavailable.",
  maintenance: "Computers is undergoing maintenance.",
};
const phases: Record<ComputerPhase, string> = {
  reserved: "Reserved",
  provisioning: "Preparing home",
  starting: "Starting",
  ready: "Ready",
  stopping: "Stopping",
  stopped: "Stopped",
  error: "Error",
  recovery_required: "Recovery required",
};
export function ComputersPage({
  scope,
  canReadInstallation,
}: {
  scope: string;
  canReadInstallation: boolean;
}) {
  const [controller] = useState(
    () =>
      new ComputersController(scope, {
        read: readComputers,
        operation: readOperation,
        command: lifecycle,
        watch: watchComputers,
        storage: window.sessionStorage,
      }),
  );
  const state = useSyncExternalStore(controller.subscribe, controller.snapshot);
  const selectionKey = `veoveo.computers.selected:${scope}`;
  const [selectedId, select] = useState<string | undefined>(() => {
    try {
      const route = window.location.hash.match(/^#\/computers\/([^/]+)$/)?.[1];
      const candidate = z.uuid().safeParse(route ?? window.sessionStorage.getItem(selectionKey));
      return candidate.success ? candidate.data : undefined;
    } catch {
      return undefined;
    }
  });
  useEffect(() => {
    controller.start();
    return () => controller.dispose();
  }, [controller]);
  const snapshot = state.snapshot;
  const selected =
    snapshot?.computers.find((computer) => computer.computerId === selectedId) ??
    snapshot?.computers[0];
  const blocked = state.stale || !!state.persistenceError;
  return (
    <section className="computers-workspace" aria-label="Computers">
      <div className="computers-toolbar">
        <div>
          <h2>Your Computers</h2>
          <p>Work in a retained home. Stop releases compute and keeps your files.</p>
        </div>
        <div className="computers-actions">
          <button className="button button-secondary" onClick={controller.reconnect}>
            <RefreshCw size={15} /> Refresh
          </button>
          <button
            className="button button-primary"
            disabled={blocked || !snapshot?.canCreate}
            onClick={() => void controller.command("create")}
          >
            <Plus size={15} /> Create Computer
          </button>
        </div>
      </div>
      <div className="computers-status" role="status">
        {state.loading ? "Loading Computers…" : snapshot && availability[snapshot.availability]}
        {state.stale &&
          !state.loading &&
          " Live state is being re-established; actions will resume when current state is confirmed."}
      </div>
      {snapshot?.availability === "setup_required" && canReadInstallation && (
        <p>
          Configure retained storage, a template and capacity in the installation’s Computers
          settings.
        </p>
      )}
      {state.error && (
        <p role="alert" className="computers-error">
          {state.error}
        </p>
      )}
      {state.persistenceError && (
        <div role="alert" className="computers-error">
          <p>{state.persistenceError}</p>
          <button
            className="button button-secondary"
            disabled={!snapshot || state.stale}
            onClick={() => {
              if (
                window.confirm(
                  "Clear saved recovery requests? Review the Computers above first; an unconfirmed request may already have been accepted.",
                )
              )
                controller.clearSavedAfterReview();
            }}
          >
            Clear saved requests after review
          </button>
        </div>
      )}
      {snapshot?.template && (
        <p className="computers-template">
          {snapshot.template.templateId} · {snapshot.template.cpus} CPU ·{" "}
          {snapshot.template.memoryMib} MiB memory
          {snapshot.template.homeCapacityMib
            ? ` · ${snapshot.template.homeCapacityMib} MiB retained home`
            : ""}
        </p>
      )}
      <div className="computers-layout">
        <div className="computers-collection" aria-label="Your Computers">
          {snapshot && snapshot.computers.length === 0 && (
            <p>No Computers in this Work Context yet.</p>
          )}
          {snapshot?.computers.map((computer) => (
            <button
              className={`computer-card ${selected?.computerId === computer.computerId ? "computer-selected" : ""}`}
              key={computer.computerId}
              onClick={() => {
                select(computer.computerId);
                window.history.replaceState(null, "", `#/computers/${computer.computerId}`);
                try {
                  window.sessionStorage.setItem(selectionKey, computer.computerId);
                } catch {
                  /* Selection does not carry operation authority. */
                }
              }}
              aria-pressed={selected?.computerId === computer.computerId}
            >
              <Monitor size={18} />
              <span>
                <strong>Computer {computer.computerId.slice(-8)}</strong>
                <small>
                  {phases[computer.phase]}
                  {computer.busy ? " · Operation in progress" : ""}
                </small>
              </span>
            </button>
          ))}
          {snapshot?.nextCursor && (
            <button className="button button-secondary" onClick={controller.loadMore}>
              Load more
            </button>
          )}
        </div>
        {selected && (
          <section
            className="computer-detail"
            aria-label={`Computer ${selected.computerId.slice(-8)}`}
          >
            <div className="computers-toolbar">
              <div>
                <h3>Computer {selected.computerId.slice(-8)}</h3>
                <p>
                  {phases[selected.phase]} · {selected.templateId}
                </p>
              </div>
              <div className="computers-actions">
                {selected.canCreate && (
                  <button
                    className="button button-primary"
                    disabled={blocked}
                    onClick={() => void controller.command("create", selected.computerId)}
                  >
                    Continue setup
                  </button>
                )}
                {selected.canStart && (
                  <button
                    className="button button-primary"
                    disabled={blocked}
                    onClick={() => void controller.command("start", selected.computerId)}
                  >
                    <Play size={15} /> Start
                  </button>
                )}
                {selected.canStop && (
                  <button
                    className="button button-secondary"
                    disabled={blocked}
                    onClick={() => {
                      if (
                        window.confirm(
                          "Stop this Computer? Running processes end. Its retained home and files are kept.",
                        )
                      )
                        void controller.command("stop", selected.computerId);
                    }}
                  >
                    <Square size={15} /> Stop
                  </button>
                )}
              </div>
            </div>
            <Suspense fallback={<p>Loading terminal…</p>}>
              <TerminalPanel
                key={selected.computerId}
                computerId={selected.computerId}
                canConnect={selected.canConnect}
              />
            </Suspense>
            {snapshot && <AccessPanel key={`access:${selected.computerId}`}
              computerId={selected.computerId} snapshot={snapshot} stale={state.stale} />}
            <CliConnect computerId={selected.computerId} canConnect={selected.canConnect} />
            {snapshot && snapshot.availability !== "setup_required" && <AutomationPanel key={`automation:${selected.computerId}`}
              computerId={selected.computerId} scope={scope} snapshot={snapshot} stale={state.stale} />}
            <details>
              <summary>Computer details</summary>
              <dl>
                <dt>ID</dt>
                <dd>{selected.computerId}</dd>
                <dt>Last changed</dt>
                <dd>{new Date(selected.updatedAt).toLocaleString()}</dd>
                {selected.activeTaskId && (
                  <>
                    <dt>Active operation</dt>
                    <dd>{selected.activeTaskId}</dd>
                  </>
                )}
              </dl>
            </details>
          </section>
        )}
      </div>
      {state.intents.length > 0 && (
        <section className="computer-operations" aria-label="Operation receipts">
          <h3>Recent requests</h3>
          {state.intents.map((intent) => (
            <div className="computer-operation" key={intent.requestId}>
              <div>
                <strong>{intent.action[0].toUpperCase() + intent.action.slice(1)}</strong>
                <p role="status">
                  {intent.sending
                    ? "Submitting the saved request…"
                    : (intent.error ??
                      (intent.receipt
                        ? `Last confirmed operation state: ${intent.receipt.status.replaceAll("_", " ")}`
                        : "Outcome unconfirmed. Retry this saved request to recover its state."))}
                </p>
                <small>
                  Request {intent.requestId}
                  {intent.receipt ? ` · Operation ${intent.receipt.taskId}` : ""}
                </small>
              </div>
              <button
                className="button button-secondary"
                disabled={intent.sending}
                onClick={() => void controller.retry(intent.requestId)}
              >
                Recover status
              </button>
              {intent.receipt && (
                <button
                  className="button button-secondary"
                  disabled={intent.sending}
                  onClick={() => controller.dismiss(intent.requestId)}
                >
                  Dismiss receipt
                </button>
              )}
            </div>
          ))}
        </section>
      )}
    </section>
  );
}
