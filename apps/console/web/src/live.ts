import { useEffect, useState } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { attachAppCatalogEvents } from "./apps/catalogEvents";
import { queryKeys } from "./queries";
import { ENTITY_EVENTS, applySnapshotEvent, parseEntityEvent, type EntityEvent } from "./consoleEvents";
import { parseConsole } from "./generatedContracts";
import { uploadNotificationSchema } from "./uploads/model";
import type {
  AppCatalog,
  InstallationSnapshot,
} from "./types";

export type LiveStatus = "live" | "reconnecting" | "off";

export { applyRowEvent };
function applyRowEvent(client: QueryClient, entity: EntityEvent, value: unknown): void {
  const message = parseEntityEvent(entity, value);
  client.setQueryData<InstallationSnapshot>(queryKeys.snapshot, (snapshot) =>
    snapshot ? applySnapshotEvent(snapshot, message) : snapshot);
}

const RESYNC_AFTER_FAILURES = 3;

/**
 * Live console updates: an EventSource against the BFF stream proxy feeding
 * row upserts straight into the snapshot query cache. The browser
 * auto-reconnects with `Last-Event-ID`; a `reset` event or repeated
 * failures force a snapshot refetch, whose fresh cursor restarts the
 * stream via the effect dependency.
 */
export function useConsoleLiveStream(cursor: string | undefined, reconcileUploads?: (uploadId?: string) => void): LiveStatus {
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
      reconcileUploads?.();
    };
    source.onerror = () => {
      setStatus("reconnecting");
      failures += 1;
      if (failures >= RESYNC_AFTER_FAILURES) resync();
    };
    for (const entity of ENTITY_EVENTS) {
      source.addEventListener(entity, (event) => {
        try {
          applyRowEvent(client, entity, JSON.parse((event as MessageEvent<string>).data));
        } catch { /* Rejected events leave the snapshot unchanged. */ }
      });
    }
    source.addEventListener("access_request", (event) => {
      try {
        parseConsole("accessRequestEvent", JSON.parse((event as MessageEvent<string>).data));
        void client.invalidateQueries({ queryKey: queryKeys.accessRequests });
      } catch { /* A malformed notification cannot drive reconciliation. */ }
    });
    source.addEventListener("artifact_upload", (event) => {
      try {
        const notification = uploadNotificationSchema.safeParse(JSON.parse((event as MessageEvent<string>).data));
        if (notification.success) reconcileUploads?.(notification.data.upload_id);
      } catch { /* A malformed event cannot establish an upload receipt. */ }
    });
    source.addEventListener("reset", (event) => {
      try {
        parseConsole("resetEvent", JSON.parse((event as MessageEvent<string>).data));
        resync();
      } catch { /* Only an admitted reset can restart the stream. */ }
    });

    return () => {
      disposed = true;
      source.close();
      setStatus("off");
    };
  }, [cursor, client, reconcileUploads]);

  return status;
}

/** Keep the MCP App catalog synchronized with gateway list-change events. */
export function useAppCatalogLive(enabled: boolean): void {
  const client = useQueryClient();

  useEffect(() => {
    if (!enabled || import.meta.env.VITE_DEMO_DATA === "true") return;
    const source = new EventSource("/console/api/apps/events");
    return attachAppCatalogEvents(source, (catalog) => {
      client.setQueryData<AppCatalog>(queryKeys.apps, catalog);
    });
  }, [client, enabled]);
}
