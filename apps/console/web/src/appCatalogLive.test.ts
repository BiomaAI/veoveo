import assert from "node:assert/strict";
import test from "node:test";

import { attachAppCatalogEvents } from "./apps/catalogEvents.ts";
import type { AppCatalog } from "./types.ts";

const catalog: AppCatalog = {
  apps: [{
    server: "map", resourceUri: "ui://map/workspace.html", standalonePath: "/apps/map/workspace",
    name: "Map Explorer", resourceDependencies: [], toolDependencies: [], agentMessageTargets: [],
    tools: [{
      name: "find", inputSchema: {
        type: "object", properties: { vendor_specific_field: { type: "string" } },
      },
    }],
  }],
  degradations: [{ server: "media", surface: "tools", code: "upstream_unavailable" }],
};

function stream(receive: (value: AppCatalog) => void) {
  let listener: EventListenerOrEventListenerObject | undefined;
  let closed = false;
  const source = {
    addEventListener(type: string, candidate: EventListenerOrEventListenerObject) {
      assert.equal(type, "catalog");
      listener = candidate;
    },
    close() {
      closed = true;
    },
  } as Pick<EventSource, "addEventListener" | "close">;
  const cleanup = attachAppCatalogEvents(source, receive);
  assert.ok(listener);
  return {
    cleanup,
    closed: () => closed,
    emit(data: string) {
      const event = new MessageEvent("catalog", { data });
      if (typeof listener === "function") listener(event);
      else listener!.handleEvent(event);
    },
  };
}

test("catalog events publish admitted snapshots and cleanup closes the stream", () => {
  const received: AppCatalog[] = [];
  const source = stream((value) => received.push(value));
  source.emit(JSON.stringify(catalog));
  source.emit(JSON.stringify({ apps: [], degradations: [] }));
  assert.deepEqual(received, [catalog, { apps: [], degradations: [] }]);
  source.cleanup();
  assert.equal(source.closed(), true);
});

test("malformed catalog events cannot reach the receiver or replace its current snapshot", () => {
  const received: AppCatalog[] = [];
  const source = stream((value) => received.push(value));
  source.emit(JSON.stringify(catalog));
  const app = catalog.apps[0];
  const { resourceUri, ...retired } = app;
  const malformed = [
    "{private-synthetic-payload",
    JSON.stringify(null),
    JSON.stringify({ apps: [], degradations: "invalid" }),
    JSON.stringify({ apps: [app] }),
    JSON.stringify({ ...catalog, apps: [{ ...app, resourceUri: 42 }] }),
    JSON.stringify({ ...catalog, apps: [{ ...retired, resource_uri: resourceUri }] }),
    JSON.stringify({ ...catalog, apps: [{ ...app, tools: [{ name: 42, inputSchema: {} }] }] }),
    JSON.stringify({ ...catalog, degradations: [{ server: "map", surface: "unknown", code: "upstream_unavailable" }] }),
    JSON.stringify({ ...catalog, degradations: [{ server: "map", surface: "tools", code: "unknown" }] }),
  ];
  for (const data of malformed) {
    assert.throws(() => source.emit(data), { message: "Invalid App catalog event" });
    assert.deepEqual(received, [catalog]);
    assert.equal(source.closed(), false);
  }
  // A rejected event leaves the existing listener usable; later valid data is admitted.
  source.emit(JSON.stringify({ apps: [], degradations: [] }));
  assert.deepEqual(received, [catalog, { apps: [], degradations: [] }]);
  source.cleanup();
  assert.equal(source.closed(), true);
});
