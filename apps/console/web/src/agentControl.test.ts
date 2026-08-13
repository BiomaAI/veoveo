import assert from "node:assert/strict";
import test from "node:test";
import {
  agentDisplayState,
  agentInputRequestDecisionPath,
  agentInputRequestsApiPath,
  agentInputRequestsPath,
  uuidV7,
} from "./agentControl.ts";

test("input request paths retain and encode the symbolic agent key", () => {
  const agentKey = "mission-supervisor";
  const inputRequestId = "019fd9bc-e7d1-7fff-bfff-ffffffffffff";
  assert.equal(agentInputRequestsPath(agentKey), `agents/${agentKey}/input-requests`);
  assert.equal(agentInputRequestsApiPath(agentKey), `/console/api/agents/${agentKey}/input-requests`);
  assert.equal(
    agentInputRequestDecisionPath(agentKey, inputRequestId),
    `agents/${agentKey}/input-requests/${inputRequestId}/decision`,
  );
  assert.equal(
    agentInputRequestsPath("supervisor/primary"),
    "agents/supervisor%2Fprimary/input-requests",
  );
});

test("agent display state marks an unleased or expired runner offline", () => {
  const expiry = new Date(2_000).toISOString();
  assert.equal(agentDisplayState({ state: "running", runnerLeaseExpiresAt: expiry }, 1_999), "running");
  assert.equal(agentDisplayState({ state: "running", runnerLeaseExpiresAt: expiry }, 2_000), "offline");
  assert.equal(agentDisplayState({ state: "running" }, 1_000), "offline");
  assert.equal(agentDisplayState({ state: "disabled" }, 1_000), "disabled");
  assert.equal(agentDisplayState({ state: "failed" }, 1_000), "failed");
});

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
