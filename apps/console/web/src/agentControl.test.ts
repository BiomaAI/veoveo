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


test("owner Agent wire schemas reject retired and mixed keys before client dispatch", async () => {
  const { parseAgentControl } = await import("./generatedContracts.ts");
  const requestId = "019fd9bc-e7d1-7fff-bfff-ffffffffffff";
  const current = {
    agentId: "mission-supervisor",
    entries: [{ entryId: `wake:${requestId}`, role: "operator", actorId: "operator",
      content: "inspect route", state: "accepted", occurredAt: "2026-10-06T00:00:00Z",
      requestId, wakeId: requestId, inReplyToRequestIds: [requestId] }],
  };
  assert.deepEqual(parseAgentControl("conversation", current), current);
  for (const [parent, key, retired] of [
    ["", "agentId", "agent_id"], ["entry", "entryId", "entry_id"],
    ["entry", "actorId", "actor_id"], ["entry", "occurredAt", "occurred_at"],
    ["entry", "requestId", "request_id"], ["entry", "wakeId", "wake_id"],
    ["entry", "inReplyToRequestIds", "in_reply_to_request_ids"],
  ]) {
    for (const keepCurrent of [false, true]) {
      const changed = structuredClone(current);
      const target: Record<string, unknown> = parent === "entry" ? changed.entries[0] : changed;
      target[retired] = target[key];
      if (!keepCurrent) delete target[key];
      assert.throws(() => parseAgentControl("conversation", changed), `${retired}: mixed=${keepCurrent}`);
    }
  }
  for (const kind of ["message", "decision"] as const) {
    const value = kind === "message" ? { requestId, message: "inspect" } : { requestId, action: "decline" };
    assert.deepEqual(parseAgentControl(kind, value), value);
    assert.throws(() => parseAgentControl(kind, { ...value, request_id: requestId }));
    const rest: Record<string, unknown> = { ...value };
    delete rest.requestId;
    assert.throws(() => parseAgentControl(kind, { ...rest, request_id: requestId }));
  }
});
