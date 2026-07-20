import { useEffect, useState } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { queryKeys } from "./queries";
import type {
  AgentSummary,
  ArtifactSummary,
  AuditSummary,
  InstallationSnapshot,
  McpServerSummary,
  RecordingSummary,
  TaskSummary,
} from "./types";

export type LiveStatus = "live" | "reconnecting" | "off";

const ENTITY_EVENTS = ["task", "artifact", "agent", "recording", "audit", "server"] as const;
type EntityEvent = (typeof ENTITY_EVENTS)[number];

type RowEvent =
  | { op: "upsert"; row: Record<string, unknown> }
  | { op: "delete"; id: string };

const ROW_CAP = 500;
const AUDIT_ROW_CAP = 400;

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

export function applyRowEvent(client: QueryClient, entity: EntityEvent, event: RowEvent): void {
  client.setQueryData<InstallationSnapshot>(queryKeys.snapshot, (snapshot) => {
    if (!snapshot) return snapshot;
    switch (entity) {
      case "task":
        return {
          ...snapshot,
          tasks:
            event.op === "upsert"
              ? upsertSorted(snapshot.tasks, event.row as unknown as TaskSummary, (task) => task.updatedAt, ROW_CAP)
              : removeRow(snapshot.tasks, event.id),
        };
      case "artifact":
        return {
          ...snapshot,
          artifacts:
            event.op === "upsert"
              ? upsertSorted(
                  snapshot.artifacts,
                  event.row as unknown as ArtifactSummary,
                  (artifact) => artifact.createdAt,
                  ROW_CAP
                )
              : removeRow(snapshot.artifacts, event.id),
        };
      case "agent":
        return {
          ...snapshot,
          agents:
            event.op === "upsert"
              ? upsertKeyed(snapshot.agents, event.row as unknown as AgentSummary, ROW_CAP)
              : removeRow(snapshot.agents, event.id),
        };
      case "recording":
        return {
          ...snapshot,
          recordings:
            event.op === "upsert"
              ? upsertSorted(
                  snapshot.recordings,
                  event.row as unknown as RecordingSummary,
                  (recording) => recording.startedAt,
                  ROW_CAP
                )
              : removeRow(snapshot.recordings, event.id),
        };
      case "audit":
        return {
          ...snapshot,
          audit:
            event.op === "upsert"
              ? upsertSorted(
                  snapshot.audit,
                  event.row as unknown as AuditSummary,
                  (item) => item.occurredAt,
                  AUDIT_ROW_CAP
                )
              : removeRow(snapshot.audit, event.id),
        };
      case "server":
        return {
          ...snapshot,
          servers:
            event.op === "upsert"
              ? upsertKeyed(snapshot.servers, event.row as unknown as McpServerSummary, ROW_CAP)
              : removeRow(snapshot.servers, event.id),
        };
    }
  });
}

const RESYNC_AFTER_FAILURES = 3;

/**
 * Live console updates: an EventSource against the BFF stream proxy feeding
 * row upserts straight into the snapshot query cache. The browser
 * auto-reconnects with `Last-Event-ID`; a `reset` event or repeated
 * failures force a snapshot refetch, whose fresh cursor restarts the
 * stream via the effect dependency.
 */
export function useConsoleLiveStream(cursor: string | undefined): LiveStatus {
  const client = useQueryClient();
  const [status, setStatus] = useState<LiveStatus>("off");

  useEffect(() => {
    if (!cursor || import.meta.env.VITE_DEMO_DATA === "true") return;
    let disposed = false;
    let failures = 0;
    const source = new EventSource(`/console/api/stream?cursor=${encodeURIComponent(cursor)}`);

    const resync = () => {
      source.close();
      if (disposed) return;
      setStatus("reconnecting");
      void client.invalidateQueries({ queryKey: queryKeys.snapshot });
    };

    source.onopen = () => {
      failures = 0;
      setStatus("live");
    };
    source.onerror = () => {
      setStatus("reconnecting");
      failures += 1;
      if (failures >= RESYNC_AFTER_FAILURES) resync();
    };
    for (const entity of ENTITY_EVENTS) {
      source.addEventListener(entity, (event) => {
        applyRowEvent(client, entity, JSON.parse((event as MessageEvent<string>).data) as RowEvent);
      });
    }
    source.addEventListener("access_request", () => {
      void client.invalidateQueries({ queryKey: queryKeys.accessRequests });
    });
    source.addEventListener("reset", resync);

    return () => {
      disposed = true;
      source.close();
      setStatus("off");
    };
  }, [cursor, client]);

  return status;
}
