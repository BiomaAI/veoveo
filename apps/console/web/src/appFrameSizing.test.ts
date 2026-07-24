import assert from "node:assert/strict";
import test from "node:test";

import { appFrameOuterHeight } from "./appFrameSizing.ts";

test("preserves the content height reported by a bordered app frame", () => {
  assert.equal(appFrameOuterHeight(900, 2), 902);
});

test("bounds app content before adding non-content frame chrome", () => {
  assert.equal(appFrameOuterHeight(100, 2), 182);
  assert.equal(appFrameOuterHeight(1600, 2), 1402);
});

test("does not subtract malformed negative frame chrome", () => {
  assert.equal(appFrameOuterHeight(900, -2), 900);
});
