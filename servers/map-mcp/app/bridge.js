// MCP Apps control messages stay on the parent window; resource subscriptions
// use the MCP 2026-07-28 acknowledgment and notification names.
export function createBridge(host = window, timeoutMs = 15000) {
  let nextId = 1;
  let closed = false;
  const pending = new Map();
  const handlers = new Map();
  const post = (message) => host.parent.postMessage(message, "*");
  const receive = (event) => {
    if (event.source !== host.parent) return;
    const message = event.data;
    if (!message || message.jsonrpc !== "2.0") return;
    if (message.method === "notifications/subscriptions/acknowledged") {
      const id = message.params?._meta?.["io.modelcontextprotocol/subscriptionId"];
      const waiter = pending.get(id);
      if (waiter?.stream) {
        clearTimeout(waiter.timer);
        waiter.acknowledged = true;
        waiter.resolve(message.params);
      }
      return;
    }
    if (message.id !== undefined && (message.result !== undefined || message.error !== undefined)) {
      const waiter = pending.get(message.id);
      if (!waiter) return;
      pending.delete(message.id);
      clearTimeout(waiter.timer);
      if (waiter.stream) {
        const error = new Error(message.error?.message || "Live updates ended. Press Refresh to reconnect.");
        if (waiter.acknowledged) waiter.onError?.(error);
        else waiter.reject(error);
      } else if (message.error) waiter.reject(new Error(message.error.message || "host error"));
      else waiter.resolve(message.result);
      return;
    }
    handlers.get(message.method)?.(message.params, message.id);
  };
  host.addEventListener("message", receive);
  return {
    request(method, params, { onError } = {}) {
      if (closed) return Promise.reject(new Error("The map connection has closed."));
      return new Promise((resolve, reject) => {
        const id = nextId++;
        const timer = setTimeout(() => {
          pending.delete(id);
          reject(new Error("The Console did not respond. Press Refresh to check the current state."));
        }, timeoutMs);
        pending.set(id, { resolve, reject, timer, stream: method === "subscriptions/listen", onError });
        post({ jsonrpc: "2.0", id, method, params });
      });
    },
    notify: (method, params) => post({ jsonrpc: "2.0", method, params }),
    on: (method, handler) => handlers.set(method, handler),
    post,
    close() {
      closed = true;
      host.removeEventListener("message", receive);
      for (const waiter of pending.values()) {
        clearTimeout(waiter.timer);
        waiter.reject(new Error("The map connection has closed."));
      }
      pending.clear();
    },
  };
}
