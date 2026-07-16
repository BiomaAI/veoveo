import { useState } from "react";
import { DrawerShell } from "./DrawerShell";
import { ProgressBar, StatusPill } from "../components/primitives";
import { formatDate } from "../format";
import { useCancelTask } from "../queries";
import type { TaskSummary } from "../types";

export function TaskDrawer({ task, onClose }: { task: TaskSummary; onClose: () => void }) {
  const cancelTask = useCancelTask();
  const [actionError, setActionError] = useState<string>();
  const cancellable = ["queued", "running", "waiting", "cancel_requested"].includes(task.state);
  const cancel = async () => {
    setActionError(undefined);
    try {
      await cancelTask.mutateAsync(task.id);
    } catch (cause) {
      setActionError(cause instanceof Error ? cause.message : "Task cancellation failed");
    }
  };
  return (
    <DrawerShell title={task.type} subtitle="Task" onClose={onClose}>
      <div className="drawer-body">
        <div className="drawer-status"><StatusPill value={task.state} /><span>{task.server}</span><span>{task.owner}</span></div>
        {actionError && <div className="action-error">{actionError}</div>}
        <section>
          <h3>Progress</h3>
          <ProgressBar value={task.progress} />
          <p className="detail-copy">{task.message ?? "No status message"}</p>
        </section>
        <section>
          <h3>Execution</h3>
          <dl className="definition-list compact">
            <div><dt>Task ID</dt><dd className="mono hash">{task.id}</dd></div>
            <div><dt>Recovery</dt><dd><span className="code-label">{task.recoveryClass}</span></dd></div>
            <div><dt>Created</dt><dd>{formatDate(task.createdAt)}</dd></div>
            <div><dt>Updated</dt><dd>{formatDate(task.updatedAt)}</dd></div>
            <div><dt>Result artifact</dt><dd className="mono">{task.resultArtifactId ?? "-"}</dd></div>
          </dl>
        </section>
      </div>
      <footer className="drawer-footer">
        <button className="button button-secondary" disabled={!cancellable || cancelTask.isPending} onClick={() => void cancel()}>
          Cancel task
        </button>
      </footer>
    </DrawerShell>
  );
}
