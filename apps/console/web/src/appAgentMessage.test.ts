import assert from "node:assert/strict";
import test from "node:test";
import { appAgentMessageRequest } from "./apps/agentMessage.ts";

const app = { agentMessageTargets: ["workflow-coordinator"] };
const requestId = "019fd9bc-e7d1-7fff-bfff-ffffffffffff";

test("App agent messaging admits only an exact declared target", () => {
  assert.deepEqual(
    appAgentMessageRequest(app, {
      agentId: "workflow-coordinator",
      requestId,
      message: "review the pending batch",
    }),
    {
      agentId: "workflow-coordinator",
      requestId,
      message: "review the pending batch",
    },
  );
  assert.equal(
    appAgentMessageRequest(app, {
      agentId: "another-agent",
      requestId,
      message: "review the pending batch",
    }),
    undefined,
  );
});

test("App agent messaging requires UUIDv7 and bounded nonempty text", () => {
  for (const value of [
    { agentId: "workflow-coordinator", requestId: crypto.randomUUID(), message: "hello" },
    { agentId: "workflow-coordinator", requestId, message: "" },
    { agentId: "workflow-coordinator", requestId, message: "x".repeat(16 * 1024 + 1) },
    { agentId: "workflow-coordinator", requestId, message: "hello", extra: true },
  ]) {
    assert.equal(appAgentMessageRequest(app, value), undefined);
  }
});
