import test from "node:test";
import assert from "node:assert/strict";
import { appResult, taskDetail, taskAck } from "./appProtocol.ts";

const seed = { resultType: "task", taskId: "opaque/not-a-uuid", status: "working", createdAt: "2026-09-15T00:00:00Z", lastUpdatedAt: "2026-09-15T00:00:00Z", ttlMs: null, _meta: { "fixture/extension": "retained" } };
test("the App adapter preserves opaque Tasks and distinguishes seed, detail and acknowledgement", () => {
  assert.deepEqual(appResult.parse(seed), seed);
  const detail = { ...seed, resultType: "complete", status: "input_required", inputRequests: { "round-1": { method: "elicitation/create", params: { mode: "form", message: "Confirm", requestedSchema: { type: "object", properties: {} } } } } };
  assert.deepEqual(taskDetail.parse(detail), detail);
  assert.throws(() => appResult.parse(detail));
  assert.throws(() => taskDetail.parse({ resultType: "complete" }));
  assert.deepEqual(taskAck.parse({ resultType: "complete" }), { resultType: "complete" });
  assert.throws(() => appResult.parse({ ...seed, taskId: "" }));
});
test("multi-round state and domain errors retain their native discriminators", () => {
  const input = { resultType: "input_required", requestState: "workspace-operation:opaque:2", inputRequests: {} };
  assert.deepEqual(appResult.parse(input), input);
  const complete = { resultType: "complete", content: [{ type: "text", text: "Domain refused the action" }], isError: true, structuredContent: { code: "denied" } };
  assert.deepEqual(appResult.parse(complete), complete);
  assert.throws(() => appResult.parse({ resultType: "complete" }));
});
