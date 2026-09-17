import { createContext, useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { parse } from "./api.ts";
import { emptyPersonal, observePersonal, personalAttention } from "./personal.ts";

export const PersonalUpdates = createContext({ connected: false, limited: false, watched: new Set<string>() });

export function usePersonalEvents(readingActivity: boolean) {
  const client = useQueryClient();
  const [state, setState] = useState(emptyPersonal);
  const [connected, setConnected] = useState(false);
  const reading = useRef(readingActivity);
  reading.current = readingActivity;
  useEffect(() => { if (readingActivity) setState(current => ({ ...current, unread: new Set() })); }, [readingActivity]);
  useEffect(() => {
    const source = new EventSource("/workspace/api/events");
    let timer: ReturnType<typeof setTimeout> | undefined;
    let inventory = false;
    const operations = new Set<string>();
    let revisions = new Map<string, string>();
    const taskVersions = new Map<string, string>();
    function refresh(all = false, id?: string) {
      inventory ||= all; if (id) operations.add(id);
      if (timer) return;
      timer = setTimeout(() => {
        timer = undefined;
        if (inventory) {
          void client.invalidateQueries({ queryKey: ["operations"] });
          void client.invalidateQueries({ queryKey: ["invitations"] });
          inventory = false;
        }
        for (const id of operations) void client.invalidateQueries({ queryKey: ["operation", id] });
        operations.clear();
      }, 200);
    }
    source.onopen = () => { setConnected(true); refresh(true); };
    source.addEventListener("personal", message => {
      try {
        const event = parse("PersonalEvent", JSON.parse(message.data));
        setState(current => {
          const next = observePersonal(current, event);
          return reading.current ? { ...next, unread: new Set() } : next;
        });
        if (event.kind === "inventory") {
          refresh(true);
          for (const operation of event.operations) if (revisions.get(operation.id) !== `${operation.revision}:${operation.phase}`) refresh(false, operation.id);
          revisions = new Map(event.operations.map(operation => [operation.id, `${operation.revision}:${operation.phase}`]));
          for (const id of taskVersions.keys()) if (!revisions.has(id)) taskVersions.delete(id);
        }
        if (event.kind === "task") {
          const version = `${event.state}:${event.updatedAt}`;
          if (taskVersions.get(event.operation) !== version) refresh(false, event.operation);
          taskVersions.set(event.operation, version);
        }
        if (event.kind === "task_unavailable") { taskVersions.delete(event.operation); refresh(false, event.operation); }
      } catch { setConnected(false); refresh(true); }
    });
    source.onerror = () => {
      setConnected(false);
      void client.invalidateQueries({ queryKey: ["session"] });
      refresh(true);
    };
    source.addEventListener("expired", () => {
      setConnected(false); setState(emptyPersonal());
      void client.invalidateQueries({ queryKey: ["session"] });
      refresh(true);
    });
    const visible = () => { if (document.visibilityState === "visible") refresh(true); };
    document.addEventListener("visibilitychange", visible);
    return () => { source.close(); if (timer) clearTimeout(timer); document.removeEventListener("visibilitychange", visible); };
  }, [client]);
  const watched = useMemo(() => new Set(state.liveTasks && connected ? [...state.items].filter(([, item]) => !!item.task).map(([id]) => id) : []), [state, connected]);
  return { connected, limited: state.limited, watched, attention: personalAttention(state), unread: state.unread.size };
}
