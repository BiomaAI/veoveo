import { EmptyState, ProgressBar, RowLink, StatusPill } from "./primitives";
import { formatDate } from "../format";
import type { TaskSummary } from "../types";

export function TaskTable({
  tasks,
  onSelect,
  compact = false
}: {
  tasks: TaskSummary[];
  onSelect: (task: TaskSummary) => void;
  compact?: boolean;
}) {
  if (!tasks.length) return <EmptyState>No tasks match the current view.</EmptyState>;
  return (
    <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>Task</th>
            <th>State</th>
            <th>Progress</th>
            {!compact && <th>Recovery</th>}
            <th>Owner</th>
            <th>Updated</th>
            <th aria-label="Open" />
          </tr>
        </thead>
        <tbody>
          {tasks.map((task) => (
            <tr key={task.id} onClick={() => onSelect(task)} tabIndex={0}>
              <td>
                <strong>{task.type}</strong>
                <span className="mono subdued">{task.id.slice(0, 13)}… · {task.server}</span>
              </td>
              <td><StatusPill value={task.state} /></td>
              <td><ProgressBar value={task.progress} /></td>
              {!compact && <td><span className="code-label">{task.recoveryClass}</span></td>}
              <td>{task.owner}</td>
              <td>{formatDate(task.updatedAt)}</td>
              <td><RowLink /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
