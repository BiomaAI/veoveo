import assert from "node:assert/strict";
import test from "node:test";

import { attachAppCatalogEvents } from "./apps/catalogEvents.ts";

test("catalog events publish snapshots and cleanup closes the stream", () => {
  let listener: EventListenerOrEventListenerObject | undefined;
  let apps = 0;
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

  const cleanup = attachAppCatalogEvents(source, (catalog) => {
    apps = catalog.apps.length;
  });
  assert.ok(listener);
  const event = new MessageEvent("catalog", {
    data: JSON.stringify({ apps: [{ resourceUri: "ui://map/workspace.html" }], degradations: [] }),
  });
  if (typeof listener === "function") listener(event);
  else listener.handleEvent(event);

  assert.equal(apps, 1);
  cleanup();
  assert.equal(closed, true);
});
