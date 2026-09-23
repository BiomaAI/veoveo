import { browserJson } from "../../console/web/src/browserHttp.ts";
import { browserSession } from "../../console/web/src/csrf.ts";
import type { AppDescriptor } from "../../console/web/src/types.ts";
import type { AppToolResult, AppToolRequestExtras, InputResponses } from "../../console/web/src/apps/protocol.ts";
import type { OperationSummary } from "./generated/workspace.ts";
import { parse, unreadableResponse } from "./api.ts";

import type { z } from "zod";
import { appCatalog, appResult, taskDetail, taskAck, resourceResult } from "./appProtocol.ts";

function read<T>(schema: z.ZodType<T>, value: unknown): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    console.error("Workspace could not read an app response", result.error);
    throw new Error(unreadableResponse);
  }
  return result.data;
}
export const appApi = {
  catalog: async (signal?: AbortSignal) => read(appCatalog, await browserJson("apps", undefined, signal)),
  read: async (app: AppDescriptor, uri: string, signal?: AbortSignal) => read(resourceResult, await browserJson("apps/read", { server: app.server, appUri: app.resourceUri, uri }, signal)),
  task: async (app: AppDescriptor, taskId: string, signal?: AbortSignal) => read(taskDetail, await browserJson("app-tasks/get", { appUri: app.resourceUri, taskId }, signal)),
  cancel: async (app: AppDescriptor, taskId: string, signal?: AbortSignal) => read(taskAck, await browserJson("app-tasks/cancel", { appUri: app.resourceUri, taskId }, signal)),
  update: async (app: AppDescriptor, taskId: string, inputResponses: InputResponses, signal?: AbortSignal) => read(taskAck, await browserJson("app-tasks/update", { appUri: app.resourceUri, taskId, inputResponses }, signal)),
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
  // Observe this recorded request only. Neither a lost response nor a reload
  // repeats tools/call. The Activity view handles recovery.
  const deadline = Date.now() + 90_000;
  while (!signal.aborted && Date.now() < deadline) {
    const view = parse("AppOperationView", await browserJson(`app-operations/${operation.id}`, { appUri: app.resourceUri }, signal));
    if (view.operation.id !== operation.id) throw new Error(unreadableResponse);
    if (view.native) { changed(); return read(appResult, view.native); }
    if (view.operation.phase === "failed") throw new Error("Veoveo didn't run this tool. You may not have permission to use it. Check your Activity for details.");
    if (view.operation.phase !== "dispatching") break;
    await delay(signal);
  }
  changed();
  throw new Error("This tool is still starting, or its result is unknown. Check your Activity before trying again.");
}

export async function appResourceEvents(server: string, appUri: string, subscriptions: Array<{ subscriptionId: string; uri: string }>, signal?: AbortSignal | null) {
  if (!browserSession.csrfToken) throw new Error("Your session ended. Sign in again to continue.");
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
