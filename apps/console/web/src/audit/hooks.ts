import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AuditAccessDenied, loadDailyCounts, loadPartitions, loadRecords, openAuditView, streamUrl } from "./api.ts";
import type { AuditDailyQuery, AuditPartition, AuditQuery, AuditViewSession } from "../generated/audit.ts";

export const partitionKey = (partition: AuditPartition) => JSON.stringify(partition);
export const useAuditPartitions = () => useQuery({ queryKey: ["audit-partitions"], queryFn: ({ signal }) => loadPartitions(signal), retry: false });
export function useAuditSession(partition: AuditPartition) {
  const [openId] = useState(() => crypto.randomUUID());
  return useQuery({ queryKey: ["audit-view", partitionKey(partition), openId],
    queryFn: () => openAuditView(partition), staleTime: Infinity, retry: false, refetchOnMount: false });
}
export const useAuditRecords = (query: AuditQuery, view: AuditViewSession | undefined) => useQuery({
  queryKey: ["audit", partitionKey(query.partition), "records", view?.id, query], enabled: !!view,
  queryFn: ({ signal }) => { if (!view) throw new Error("Open an audit view first."); return loadRecords(query, view.id, signal); },
  retry: false, staleTime: Infinity,
});
export const useAuditDaily = (query: AuditDailyQuery, view: AuditViewSession | undefined) => useQuery({
  queryKey: ["audit", partitionKey(query.partition), "daily", view?.id, query], enabled: !!view,
  queryFn: ({ signal }) => { if (!view) throw new Error("Open an audit view first."); return loadDailyCounts(query, view.id, signal); },
  retry: false, staleTime: Infinity,
});

type LiveStatus = "connecting" | "live" | "reconnecting" | "denied" | "expired";
export function useAuditLive(partition: AuditPartition, view: AuditViewSession | undefined): LiveStatus {
  const client = useQueryClient();
  const [status, setStatus] = useState<LiveStatus>("connecting");
  const key = partitionKey(partition);
  useEffect(() => {
    if (!view) return;
    const events = new EventSource(streamUrl(partition, view.id));
    const expiry = setTimeout(() => { events.close(); setStatus("expired"); }, Math.max(0, new Date(view.expiresAt).getTime() - Date.now()));
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let checking = false;
    const invalidate = () => {
      if (timer !== undefined) return;
      // Coalesce push notifications; this timer never initiates periodic reads.
      timer = setTimeout(() => {
        timer = undefined;
        void client.invalidateQueries({ queryKey: ["audit", key] });
      }, 100);
    };
    events.onopen = () => { setStatus("live"); invalidate(); };
    events.addEventListener("invalidate", invalidate);
    events.addEventListener("reset", invalidate);
    events.onerror = () => {
      setStatus("reconnecting");
      if (checking) return;
      checking = true;
      // EventSource hides HTTP errors. Probe once per connection failure so an
      // expired session redirects and a removed partition closes its stream.
      void loadPartitions(controller.signal).then((allowed) => {
        if (!allowed.some((value) => partitionKey(value) === key)) throw new AuditAccessDenied();
      }).catch((error: unknown) => {
        if (error instanceof AuditAccessDenied) {
          events.close(); setStatus("denied");
          void client.cancelQueries({ queryKey: ["audit", key] });
          client.removeQueries({ queryKey: ["audit", key] });
        }
      }).finally(() => { checking = false; });
    };
    return () => { events.close(); controller.abort(); clearTimeout(expiry); if (timer !== undefined) clearTimeout(timer); };
  }, [client, key, partition, view]);
  return status;
}
