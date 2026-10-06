import notificationFixture from "../../../../mcp/contract/testdata/upload-notifications.json" with { type: "json" };
import assert from "node:assert/strict";
import test from "node:test";
import { demoSnapshot } from "./demo.ts";
import { ENTITY_EVENTS, applySnapshotEvent, parseEntityEvent } from "./consoleEvents.ts";
import { parseConsole } from "./generatedContracts.ts";
import { uploadNotificationSchema } from "./uploads/model.ts";

const timestamp = "2026-10-05T12:34:56.123456789Z";
const id = "01994bed-e0d0-7000-8000-000000000001";
function populatedSnapshot() {
  // JSON encoding strips browser-only undefined values, matching HTTP admission.
  const value = JSON.parse(JSON.stringify(demoSnapshot));
  for (const service of value.services) delete service.latencyMs;
  value.tasks[0].recoveryClass = "provider_wait";
  value.tasks[0].updatedAt = timestamp;
  const artifact = value.artifacts[0];
  artifact.byteLength = null;
  delete artifact.taskId;
  artifact.grants = [{ subjectKind: "principal", subject: "actor", permission: "read", labels: [], createdAt: timestamp }];
  artifact.shareLinks = [{ id, permission: "read", expiresAt: timestamp, downloadCount: 0, createdAt: timestamp, active: true }];
  artifact.recording = { recordingId: value.recordings[0].id, kind: "recording", ordinal: 0 };
  artifact.effectiveAccess.sources = [{ kind: "principal_grant", subject: "actor", level: "read" }];
  value.servers[0].ownedRoutes = [{ path: "/health", purpose: "health" }];
  return parseConsole("snapshot", value);
}

test("populated snapshot admits all summary shapes and preserves provider wait, nullable size, omissions and timestamps", () => {
  const snapshot = populatedSnapshot();
  assert.equal(snapshot.tasks[0].recoveryClass, "provider_wait");
  assert.equal(snapshot.tasks[0].updatedAt, timestamp);
  assert.equal(snapshot.artifacts[0].byteLength, null);
  assert.equal(Object.hasOwn(snapshot.artifacts[0], "taskId"), false);
  for (const byteLength of [0, Number.MAX_SAFE_INTEGER]) {
    const candidate = structuredClone(snapshot);
    candidate.artifacts[0].byteLength = byteLength;
    assert.equal(parseConsole("snapshot", candidate).artifacts[0].byteLength, byteLength);
  }
  for (const byteLength of [-1, 0.5, Number.MAX_SAFE_INTEGER + 1]) {
    const candidate = structuredClone(snapshot);
    candidate.artifacts[0].byteLength = byteLength;
    assert.throws(() => parseConsole("snapshot", candidate));
  }
  const missingSize = JSON.parse(JSON.stringify(snapshot));
  delete missingSize.artifacts[0].byteLength;
  assert.throws(() => parseConsole("snapshot", missingSize));
  assert.throws(() => parseConsole("snapshot", { ...snapshot, installation: { ...snapshot.installation, extra: true } }));
  assert.throws(() => parseConsole("snapshot", { ...snapshot, tasks: [{ ...snapshot.tasks[0], recoveryClass: "poll" }] }));
});

const lists = { principal: "principals", task: "tasks", artifact: "artifacts", agent: "agents", recording: "recordings", server: "servers" } as const;
test("every SSE entity admits its row and delete; wrong entities and malformed payloads preserve the snapshot", () => {
  const snapshot = populatedSnapshot();
  for (const entity of ENTITY_EVENTS) {
    const row = snapshot[lists[entity]][0];
    const updated = applySnapshotEvent(snapshot, parseEntityEvent(entity, { op: "upsert", row }));
    assert.ok(updated[lists[entity]].some((value) => value.id === row.id));
    const removed = applySnapshotEvent(snapshot, parseEntityEvent(entity, { op: "delete", id: row.id }));
    assert.ok(removed[lists[entity]].every((value) => value.id !== row.id));
    const original = structuredClone(snapshot);
    for (const invalid of [
      { op: "unknown", row }, { op: "upsert", row: { id: row.id } },
      { op: "delete" }, { op: "delete", id: row.id, row },
      { op: "upsert", row: snapshot[entity === "task" ? "artifacts" : "tasks"][0] },
    ]) {
      assert.throws(() => applySnapshotEvent(snapshot, parseEntityEvent(entity, invalid)));
      assert.deepEqual(snapshot, original);
    }
  }
  assert.throws(() => parseEntityEvent("artifact", { op: "upsert", row: { ...snapshot.artifacts[0], effectiveAccess: { ...snapshot.artifacts[0].effectiveAccess, sources: [{ kind: "unknown", subject: "actor", level: "read" }] } } }));
});

test("reset and access request payloads use closed operations and reasons", () => {
  for (const reason of ["cursor-out-of-range", "seed-failed", "replay-failed"]) assert.equal(parseConsole("resetEvent", { reason }).reason, reason);
  assert.throws(() => parseConsole("resetEvent", { reason: "unknown" }));
  assert.equal(parseConsole("accessRequestEvent", { op: "changed", id }).id, id);
  assert.throws(() => parseConsole("accessRequestEvent", { op: "delete", id }));
  assert.throws(() => parseConsole("accessRequestEvent", { op: "changed", id, extra: true }));
});

test("generated upload notifications admit six wake states and cannot carry receipts or open sessions", () => {
  for (const state of ["finalizing", "verifying", "completed", "cancelled", "expired", "failed"]) assert.ok(uploadNotificationSchema.safeParse({ op: "changed", upload_id: id, state }).success);
  for (const invalid of [
    { op: "changed", upload_id: id, state: "open" },
    { op: "changed", upload_id: id, state: "completed", receipt: {} },
    { op: "delete", upload_id: id, state: "failed" },
    { op: "changed", upload_id: "bad", state: "failed" },
  ]) assert.equal(uploadNotificationSchema.safeParse(invalid).success, false);
});


test("Rust admitted aliases emit the shared canonical notification accepted by the browser schema", () => {
  assert.ok(uploadNotificationSchema.safeParse(notificationFixture.emitted).success);
  for (const upload_id of notificationFixture.invalid_ids) {
    assert.equal(uploadNotificationSchema.safeParse({ ...notificationFixture.emitted, upload_id }).success, false);
  }
});
