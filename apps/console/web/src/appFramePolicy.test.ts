import assert from "node:assert/strict";
import test from "node:test";

import { APP_FRAME_SANDBOX } from "./apps/framePolicy.ts";

test("every MCP App host uses the opaque-origin script-only sandbox", () => {
  assert.equal(APP_FRAME_SANDBOX, "allow-scripts");
  assert.equal(APP_FRAME_SANDBOX.includes("allow-same-origin"), false);
});
