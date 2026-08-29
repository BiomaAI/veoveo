import assert from "node:assert/strict";
import test from "node:test";

import type { JSONRPCMessage, Transport } from "./apps/protocol.ts";
import { interceptResourceReadRequests } from "./apps/resourceRead.ts";
import type { AppDescriptor } from "./types.ts";

const app: AppDescriptor = {
  server: "fleet",
  resourceUri: "ui://fleet/overview.html",
  standalonePath: "/apps/fleet/overview.html",
  name: "overview",
  tools: [],
  agentMessageTargets: [],
  resourceDependencies: [
    {
      app_resource: "ui://fleet/overview.html",
      server: "map",
      scheme: "map",
      uri_prefix: "map://feature/",
      required_scope: "map:read",
      operations: ["read"],
    },
  ],
  toolDependencies: [],
};

function harness() {
  const sent: JSONRPCMessage[] = [];
  const inner: Transport = {
    start: async () => {},
    send: async (message) => { sent.push(message); },
    close: async () => {},
  };
  const reads: string[] = [];
  const transport = interceptResourceReadRequests(inner, app, async (_server, _appUri, uri) => {
    reads.push(uri);
    return { contents: [{ uri, mimeType: "application/json", text: "{}" }] };
  });
  return { inner, reads, sent, transport };
}

async function settle(): Promise<void> {
  await new Promise((resolve) => setImmediate(resolve));
}

test("App resource adapter settles owned and declared reads", async () => {
  const { inner, reads, sent } = harness();
  inner.onmessage?.({
    jsonrpc: "2.0",
    id: 2,
    method: "resources/read",
    params: { uri: "fleet://session/demo" },
  });
  inner.onmessage?.({
    jsonrpc: "2.0",
    id: 3,
    method: "resources/read",
    params: { uri: "map://feature/place-1" },
  });
  await settle();
  assert.deepEqual(reads, ["fleet://session/demo", "map://feature/place-1"]);
  assert.equal(sent.length, 2);
  assert.ok(sent.every((message) => "result" in message));
});

test("App resource adapter rejects undeclared and traversing reads", async () => {
  const { inner, reads, sent } = harness();
  for (const [id, uri] of [[4, "map://features"], [5, "map://feature/../secret"]] as const) {
    inner.onmessage?.({ jsonrpc: "2.0", id, method: "resources/read", params: { uri } });
  }
  await settle();
  assert.deepEqual(reads, []);
  assert.equal(sent.length, 2);
  assert.ok(sent.every((message) => "error" in message && message.error.code === -32602));
});

test("App resource adapter forwards unrelated protocol traffic", () => {
  const { inner, transport } = harness();
  let forwarded: JSONRPCMessage | undefined;
  transport.onmessage = (message) => { forwarded = message; };
  const message: JSONRPCMessage = {
    jsonrpc: "2.0",
    id: 6,
    method: "tools/call",
    params: { name: "inspect", arguments: {} },
  };
  inner.onmessage?.(message);
  assert.deepEqual(forwarded, message);
});
