import { parseConsole } from "./generatedContracts.ts";
import type { ConsoleApi } from "./generated/console.ts";
import type { InstallationSnapshot } from "./types.ts";

export const ENTITY_EVENTS = ["principal", "task", "artifact", "agent", "recording", "server"] as const;
export type EntityEvent = (typeof ENTITY_EVENTS)[number];

type ParsedEntityEvent = {
  [Entity in EntityEvent]: { entity: Entity; event: ConsoleApi[`${Entity}Event`] }
}[EntityEvent];

export function parseEntityEvent(entity: EntityEvent, value: unknown): ParsedEntityEvent {
  switch (entity) {
    case "principal": return { entity, event: parseConsole("principalEvent", value) };
    case "task": return { entity, event: parseConsole("taskEvent", value) };
    case "artifact": return { entity, event: parseConsole("artifactEvent", value) };
    case "agent": return { entity, event: parseConsole("agentEvent", value) };
    case "recording": return { entity, event: parseConsole("recordingEvent", value) };
    case "server": return { entity, event: parseConsole("serverEvent", value) };
  }
}

const ROW_CAP = 500;

interface Keyed {
  id: string;
}

function upsertSorted<Row extends Keyed>(
  rows: Row[],
  row: Row,
  newestFirst: (row: Row) => string,
  cap: number
): Row[] {
  const next = rows.filter((existing) => existing.id !== row.id);
  next.push(row);
  next.sort((left, right) => newestFirst(right).localeCompare(newestFirst(left)));
  return next.slice(0, cap);
}

function upsertKeyed<Row extends Keyed>(rows: Row[], row: Row, cap: number): Row[] {
  const index = rows.findIndex((existing) => existing.id === row.id);
  if (index === -1) return [row, ...rows].slice(0, cap);
  const next = rows.slice();
  next[index] = row;
  return next;
}

function removeRow<Row extends Keyed>(rows: Row[], id: string): Row[] {
  return rows.filter((existing) => existing.id !== id);
}

export function applySnapshotEvent(snapshot: InstallationSnapshot, message: ParsedEntityEvent): InstallationSnapshot {
    switch (message.entity) {
      case "principal": {
        const event = message.event;
        return {
          ...snapshot,
          principals:
            event.op === "upsert"
              ? upsertKeyed(snapshot.principals, event.row, ROW_CAP)
              : removeRow(snapshot.principals, event.id),
        };
      }
      case "task": {
        const event = message.event;
        return {
          ...snapshot,
          tasks:
            event.op === "upsert"
              ? upsertSorted(snapshot.tasks, event.row, (task) => task.updatedAt, ROW_CAP)
              : removeRow(snapshot.tasks, event.id),
        };
      }
      case "artifact": {
        const event = message.event;
        return {
          ...snapshot,
          artifacts:
            event.op === "upsert"
              ? upsertSorted(
                  snapshot.artifacts,
                  event.row,
                  (artifact) => artifact.createdAt,
                  ROW_CAP
                )
              : removeRow(snapshot.artifacts, event.id),
        };
      }
      case "agent": {
        const event = message.event;
        return {
          ...snapshot,
          agents:
            event.op === "upsert"
              ? upsertKeyed(snapshot.agents, event.row, ROW_CAP)
              : removeRow(snapshot.agents, event.id),
        };
      }
      case "recording": {
        const event = message.event;
        return {
          ...snapshot,
          recordings:
            event.op === "upsert"
              ? upsertSorted(
                  snapshot.recordings,
                  event.row,
                  (recording) => recording.startedAt,
                  ROW_CAP
                )
              : removeRow(snapshot.recordings, event.id),
        };
      }
      case "server": {
        const event = message.event;
        return {
          ...snapshot,
          servers:
            event.op === "upsert"
              ? upsertKeyed(snapshot.servers, event.row, ROW_CAP)
              : removeRow(snapshot.servers, event.id),
        };
      }
    }
}
