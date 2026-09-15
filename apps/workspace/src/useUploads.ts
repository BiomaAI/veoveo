import { useEffect, useMemo, useSyncExternalStore } from "react";
import { UploadQueue } from "../../console/web/src/uploads/queue.ts";
import type { WorkspaceBootstrap } from "./generated/workspace.ts";

export function useUploads(session: WorkspaceBootstrap) {
  const { principalId, workContext, tenantId } = session;
  const queue = useMemo(() => new UploadQueue(principalId, workContext, tenantId, () => {}), [principalId, workContext, tenantId]);
  const state = useSyncExternalStore(queue.subscribe, queue.snapshot);
  useEffect(() => { void queue.initialize(); return () => queue.dispose(); }, [queue]);
  return { queue, state };
}
