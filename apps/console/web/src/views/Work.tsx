import { useMemo, useState } from "react";
import { SectionHeader } from "../components/primitives";
import { Toolbar } from "../components/Toolbar";
import { TaskTable } from "../components/TaskTable";
import type { TaskSummary } from "../types";

export function WorkView({ tasks, onSelect }: { tasks: TaskSummary[]; onSelect: (task: TaskSummary) => void }) {
  const [query, setQuery] = useState("");
  const [state, setState] = useState("all");
  const rows = useMemo(
    () =>
      tasks.filter(
        (task) =>
          (state === "all" || task.state === state) &&
          `${task.id} ${task.type} ${task.server} ${task.owner}`.toLowerCase().includes(query.toLowerCase())
      ),
    [tasks, query, state]
  );
  return (
    <section className="panel full-panel">
      <SectionHeader
        title="Tasks"
        count={rows.length}
        actions={<Toolbar query={query} setQuery={setQuery} state={state} setState={setState} placeholder="Search tasks" />}
      />
      <TaskTable tasks={rows} onSelect={onSelect} />
    </section>
  );
}
