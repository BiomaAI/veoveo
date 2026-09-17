import type { OperationPhase, PersonalEvent, TaskState } from "./generated/workspace.ts";

type Observation = { phase: OperationPhase; task?: TaskState; updatedAt?: string; fresh: boolean };
export type PersonalState = { seeded: boolean; items: Map<string, Observation>; unread: Set<string>; limited: boolean; liveTasks: boolean };
export const emptyPersonal = (): PersonalState => ({ seeded: false, items: new Map(), unread: new Set(), limited: false, liveTasks: false });
const terminal = (state?: string) => ["completed", "failed", "cancelled"].includes(state ?? "");

export function observePersonal(current: PersonalState, event: PersonalEvent): PersonalState {
  const items = new Map(current.items), unread = new Set(current.unread);
  if (event.kind === "availability") return { ...current, liveTasks: event.liveTasks };
  if (event.kind === "inventory") {
    const retained = new Set(event.operations.map(operation => operation.id));
    for (const id of items.keys()) if (!retained.has(id)) { items.delete(id); unread.delete(id); }
    for (const operation of event.operations) {
      const previous = items.get(operation.id);
      if (current.seeded && terminal(operation.phase) && previous?.phase !== operation.phase) unread.add(operation.id);
      items.set(operation.id, { phase: operation.phase, fresh: previous?.fresh ?? current.seeded,
        ...(operation.phase === "task" && previous?.phase === "task" ? { task: previous.task, updatedAt: previous.updatedAt } : {}) });
    }
    return { ...current, items, unread, seeded: true, limited: event.limited };
  }
  const previous = items.get(event.operation);
  if (!previous) return current;
  if (event.kind === "task_unavailable") {
    items.set(event.operation, { ...previous, task: undefined, updatedAt: undefined });
    unread.delete(event.operation);
  } else {
    // Replayed baselines do not invent completion alerts. A new operation or an
    // observed active -> terminal transition does, even across reconnects.
    if (terminal(event.state) && (previous.fresh || (previous.task && !terminal(previous.task)))) unread.add(event.operation);
    items.set(event.operation, { ...previous, task: event.state, updatedAt: event.updatedAt, fresh: false });
  }
  return { ...current, items, unread };
}

export function personalAttention(state: PersonalState): number {
  return [...state.items.values()].filter(item => item.phase === "input_required" || item.task === "input_required").length;
}
