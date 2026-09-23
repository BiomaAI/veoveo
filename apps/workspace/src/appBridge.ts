import { AppBridge, PostMessageTransport } from "../../console/web/src/apps/appBridge.ts";
import { interceptResourceSubscriptions } from "../../console/web/src/apps/resourceTransport.ts";
import { isJsonRpcRequest, type Transport, type Result, type InputResponses } from "../../console/web/src/apps/protocol.ts";
import type { AppDescriptor } from "../../console/web/src/types.ts";
import { appApi, callApp, appResourceEvents, unsubscribeAppResource } from "./appApi.ts";

export function attachWorkspaceApp(frame: HTMLIFrameElement, app: AppDescriptor, chat: string, changed: () => void, navigate: (uri: string) => boolean, failed: (message: string) => void) {
  if (!frame.contentWindow) throw new Error("The app isn't ready yet. Reload the page and try again.");
  const abort = new AbortController();
  const inner = new PostMessageTransport(frame.contentWindow, frame.contentWindow);
  const tasks: Transport = { start: () => inner.start(), send: message => inner.send(message), close: () => inner.close() };
  inner.onclose = () => tasks.onclose?.(); inner.onerror = error => tasks.onerror?.(error);
  inner.onmessage = (message, extra) => {
    if (!isJsonRpcRequest(message) || !["tasks/get", "tasks/update", "tasks/cancel"].includes(message.method)) {
      tasks.onmessage?.(message, extra); return;
    }
    const act = async (): Promise<Result> => {
      const id = message.params?.taskId;
      if (typeof id !== "string" || !id || id.length > 4096) throw new Error("The app sent a request without a valid task ID.");
      if (message.method === "tasks/get") return appApi.task(app, id, abort.signal);
      if (message.method === "tasks/cancel") { const result = await appApi.cancel(app, id, abort.signal); changed(); return result; }
      const responses = message.params?.inputResponses;
      if (!responses || typeof responses !== "object" || Array.isArray(responses)) throw new Error("The app sent answers in a format Workspace can't read.");
      const result = await appApi.update(app, id, responses as InputResponses, abort.signal); changed(); return result;
    };
    void act().then(result => inner.send({ jsonrpc: "2.0", id: message.id, result }), error => inner.send({ jsonrpc: "2.0", id: message.id,
      error: { code: -32603, message: error instanceof Error ? error.message : "Veoveo couldn't tell whether the task request went through. Check your Activity." } })).catch(() => {});
  };
  const subscriptions = interceptResourceSubscriptions(tasks, app, { eventsUrl: "/workspace/api/apps/resource-events", open: appResourceEvents, unsubscribe: unsubscribeAppResource });
  const bridge = new AppBridge(null, { name: "veoveo-workspace", version: "0.1.0" }, { openLinks: {}, serverTools: {}, serverResources: {} }, {
    hostContext: { theme: "light", displayMode: "inline", availableDisplayModes: ["inline"], locale: navigator.language, platform: "web", containerDimensions: { width: frame.clientWidth } },
  });
  bridge.oncalltool = async ({ name, arguments: args, inputResponses, requestState }) => {
    if (!app.tools.some(tool => tool.name === name)) throw new Error("This app can't use that tool.");
    return callApp(app, chat, name, args ?? {}, { inputResponses, requestState }, abort.signal, changed);
  };
  bridge.onreadresource = ({ uri }) => appApi.read(app, uri, abort.signal);
  bridge.onopenlink = async ({ url }) => {
    if (url.startsWith("ui://")) {
      if (!navigate(url)) throw new Error("That app isn't available to you.");
      return {};
    }
    const target = new URL(url);
    if (target.protocol !== "https:" || target.username || target.password) throw new Error("This link is not allowed.");
    if (window.confirm(`Open ${target.hostname} in a new tab?`)) window.open(target.href, "_blank", "noopener,noreferrer");
    return {};
  };
  void bridge.connect(subscriptions.transport).catch(error => { if (!abort.signal.aborted) failed(error instanceof Error ? error.message : "The app couldn't connect. Reload the page and try again."); });
  return () => { abort.abort(); subscriptions.dispose(); void bridge.close(); };
}
