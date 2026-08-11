import assert from "node:assert/strict";
import test from "node:test";
import { uuidV7 } from "./agentControl.ts";

test("uuidV7 embeds the timestamp and required version and variant", () => {
  const timestamp = 0x019f_d9bc_e7d1;
  const value = uuidV7(timestamp, new Uint8Array(16).fill(0xff));
  assert.equal(value, "019fd9bc-e7d1-7fff-bfff-ffffffffffff");
  assert.match(value, /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});

test("uuidV7 rejects invalid timestamp and entropy bounds", () => {
  assert.throws(() => uuidV7(-1, new Uint8Array(16)), /timestamp/);
  assert.throws(() => uuidV7(0, new Uint8Array(15)), /16 bytes/);
});
