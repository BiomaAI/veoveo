import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./flint-v2.mjs", import.meta.url), "utf8");

test("Ledger override follows both initial and changed host themes", () => {
  for (const required of [
    'event.source !== parent',
    'message.jsonrpc !== "2.0"',
    'message.method === "ui/notifications/host-context-changed"',
    'message.result.hostContext',
    'document.documentElement.dataset.theme = theme',
  ]) {
    assert.match(source, new RegExp(required.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});
