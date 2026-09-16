import { browserJson } from "../../console/web/src/browserHttp.ts";
import { browserSession } from "../../console/web/src/csrf.ts";
import type { AppDescriptor } from "../../console/web/src/types.ts";
import type { AppToolResult, AppToolRequestExtras, InputResponses } from "../../console/web/src/apps/protocol.ts";
import type { OperationSummary } from "./generated/workspace.ts";
import { parse } from "./api.ts";

import { appCatalog, appResult, taskDetail, taskAck, resourceResult } from "./appProtocol.ts";
export const appApi = {
  catalog: async (signal?: AbortSignal) => appCatalog.parse(await browserJson("apps", undefined, signal)),
  read: async (app: AppDescriptor, uri: string, signal?: AbortSignal) => resourceResult.parse(await browserJson("apps/read", { server: app.server, appUri: app.resourceUri, uri }, signal)),
  task: async (app: AppDescriptor, taskId: string, signal?: AbortSignal) => taskDetail.parse(await browserJson("app-tasks/get", { appUri: app.resourceUri, taskId }, signal)),
  cancel: async (app: AppDescriptor, taskId: string, signal?: AbortSignal) => taskAck.parse(await browserJson("app-tasks/cancel", { appUri: app.resourceUri, taskId }, signal)),
  update: async (app: AppDescriptor, taskId: string, inputResponses: InputResponses, signal?: AbortSignal) => taskAck.parse(await browserJson("app-tasks/update", { appUri: app.resourceUri, taskId, inputResponses }, signal)),
};

function delay(signal: AbortSignal) {
  return new Promise<void>((resolve, reject) => {
    const stop = () => { clearTimeout(timer); reject(signal.reason); };
    const timer = setTimeout(() => { signal.removeEventListener("abort", stop); resolve(); }, 1000);
    if (signal.aborted) stop(); else signal.addEventListener("abort", stop, { once: true });
  });
}

export async function callApp(app: AppDescriptor, chat: string, tool: string, args: Record<string, unknown>, extras: AppToolRequestExtras, signal: AbortSignal, changed: () => void): Promise<AppToolResult> {
  const id = crypto.randomUUID();
  let operation: OperationSummary;
  try {
    operation = parse("OperationSummary", await browserJson(`chats/${chat}/app-operations`, { id, appUri: app.resourceUri, tool, arguments: args, ...extras }, signal));
  } finally { changed(); }
  // Observe this journal admission only. Neither a lost response nor reload
  // repeats tools/call. The existing Activity view owns durable recovery.
  const deadline = Date.now() + 90_000;
  while (!signal.aborted && Date.now() < deadline) {
    const view = parse("AppOperationView", await browserJson(`app-operations/${operation.id}`, { appUri: app.resourceUri }, signal));
    if (view.operation.id !== operation.id) throw new Error("The App operation response could not be verified.");
    if (view.native) { changed(); return appResult.parse(view.native); }
    if (view.operation.phase !== "dispatching") break;
    await delay(signal);
  }
  changed();
  throw new Error("The outcome is not confirmed. Open your Activity to recover this operation before starting it again.");
}

export async function appResourceEvents(server: string, appUri: string, subscriptions: Array<{ subscriptionId: string; uri: string }>, signal?: AbortSignal | null) {
  if (!browserSession.csrfToken) throw new Error("Sign in to continue.");
  const response = await fetch("/workspace/api/apps/resource-events", { method: "POST", credentials: "same-origin", redirect: "error",
    headers: { "Accept": "text/event-stream", "Content-Type": "application/json", "X-Veoveo-CSRF-Token": browserSession.csrfToken },
    body: JSON.stringify({ server, appUri, subscriptions }), signal,
  });
  const rotated = response.headers.get("x-veoveo-csrf-token");
  if (rotated) browserSession.csrfToken = rotated;
  if (response.status === 401) window.dispatchEvent(new Event("workspace-auth-expired"));
  return response;
}
export const unsubscribeAppResource = (subscriptionId: string) => browserJson("apps/unsubscribe", { subscriptionId });
