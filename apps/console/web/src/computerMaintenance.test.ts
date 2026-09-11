import assert from "node:assert/strict";
import test from "node:test";
import { readSavedUpdate, rememberUpdate, sendSavedUpdate } from "./computers/maintenanceRequest.ts";
import { readSavedResume, rememberResume, sendSavedResume } from "./computers/recoveryRequest.ts";
import type { MaintenanceView, UpdateTemplateInput, ResumeUpdateInput } from "./generated/computers.ts";

const input: UpdateTemplateInput = {
  computerId: "0199a001-0000-7000-8000-000000000001",
  requestId: "0199a001-0000-7000-8000-000000000002",
  templateId: "development",
};
const receipt: MaintenanceView = {
  canResume: false, pendingCancellationAt: null,
  computerId: input.computerId, taskId: "0199a001-0000-7000-8000-000000000003",
  sourceTemplateId: "development-retained", targetTemplateId: "development",
  phase: "queued", recovery: null,
  createdAt: "2026-09-10T00:00:00Z", updatedAt: "2026-09-10T00:00:00Z",
};
class MemoryStorage {
  values = new Map<string, string>();
  getItem(key: string) { return this.values.get(key) ?? null; }
  setItem(key: string, value: string) { this.values.set(key, value); }
  removeItem(key: string) { this.values.delete(key); }
}
test("reload after a lost update reply keeps the original request and template", async () => {
  const storage = new MemoryStorage();
  const calls: UpdateTemplateInput[] = [];
  await assert.rejects(sendSavedUpdate(storage, "scope", input, async sent => {
    assert.deepEqual(readSavedUpdate(storage, "scope", input.computerId)?.input, sent);
    calls.push(sent); throw new Error("lost response");
  }));
  const restored = readSavedUpdate(storage, "scope", input.computerId)!;
  assert.deepEqual(restored.input, input);
  assert.equal(restored.receipt, undefined);
  await assert.rejects(sendSavedUpdate(storage, "scope", { ...input, templateId: "another" }, async () => {
    assert.fail("a conflicting target reached the service");
  }));
  const result = await sendSavedUpdate(storage, "scope", restored.input, async sent => {
    calls.push(sent); return receipt;
  });
  assert.deepEqual(calls, [input, input]);
  assert.deepEqual(result, { receipt, saved: true });
  assert.deepEqual(readSavedUpdate(storage, "scope", input.computerId)?.receipt, receipt);
});
test("storage failure and cross-Computer saved state prevent update dispatch", async () => {
  const storage = new MemoryStorage();
  storage.setItem = () => { throw new Error("storage unavailable"); };
  await assert.rejects(sendSavedUpdate(storage, "scope", input, async () => {
    assert.fail("dispatch preceded durable browser intent");
  }));
  const other = new MemoryStorage();
  rememberUpdate(other, "scope", input);
  assert.throws(() => readSavedUpdate(other, "scope", receipt.taskId));
});
test("a late update reply cannot recreate a dismissed browser intent", async () => {
  const storage = new MemoryStorage();
  await sendSavedUpdate(storage, "scope", input, async () => {
    storage.removeItem("scope"); return receipt;
  });
  assert.equal(storage.getItem("scope"), null);
});
test("a confirmed update survives local receipt persistence failure", async () => {
  const storage = new MemoryStorage();
  const reply = await sendSavedUpdate(storage, "scope", input, async () => {
    storage.setItem = () => { throw new Error("quota exceeded"); };
    return receipt;
  });
  assert.deepEqual(reply, { receipt, saved: false });
  assert.deepEqual(readSavedUpdate(storage, "scope", input.computerId)?.input, input);
});

const resume: ResumeUpdateInput = { computerId: input.computerId, taskId: receipt.taskId,
  requestId: "0199a001-0000-7000-8000-000000000004", expectedUpdatedAt: receipt.updatedAt,
  acknowledgedCancellationAt: "2026-09-10T00:00:01Z" };
test("lost recovery replies retain the exact paused epoch and cancellation consent", async () => {
  const storage = new MemoryStorage();
  const calls: ResumeUpdateInput[] = [];
  await assert.rejects(sendSavedResume(storage, "recovery", resume, async sent => {
    assert.deepEqual(readSavedResume(storage, "recovery", resume.computerId, resume.taskId), sent);
    calls.push(sent); throw new Error("lost response");
  }));
  const restored = readSavedResume(storage, "recovery", resume.computerId, resume.taskId)!;
  assert.deepEqual(restored, resume);
  await assert.rejects(sendSavedResume(storage, "recovery", { ...resume, acknowledgedCancellationAt: null }, async () => assert.fail("changed consent dispatched")));
  await assert.rejects(sendSavedResume(storage, "recovery", { ...resume, expectedUpdatedAt: "2026-09-10T00:00:02Z" }, async () => assert.fail("changed epoch dispatched")));
  const result = await sendSavedResume(storage, "recovery", restored, async sent => { calls.push(sent); return receipt; });
  assert.deepEqual(calls, [resume, resume]);
  assert.deepEqual(result, { receipt, cleared: true });
  assert.equal(storage.getItem("recovery"), null);
});
test("recovery is bound to its Computer and Task and persists before dispatch", async () => {
  const storage = new MemoryStorage();
  storage.setItem = () => { throw new Error("storage denied"); };
  await assert.rejects(sendSavedResume(storage, "recovery", resume, async () => assert.fail("unsaved dispatch")));
  const other = new MemoryStorage();
  rememberResume(other, "recovery", resume);
  assert.throws(() => readSavedResume(other, "recovery", resume.taskId, resume.taskId));
  assert.throws(() => readSavedResume(other, "recovery", resume.computerId, resume.computerId));
  await assert.rejects(sendSavedResume(other, "recovery", resume, async () => ({ ...receipt, taskId: resume.requestId })));
  assert.deepEqual(readSavedResume(other, "recovery", resume.computerId, resume.taskId), resume);
});
test("a known recovery reply survives storage failure and cannot remove newer intent", async () => {
  const storage = new MemoryStorage();
  const confirmed = await sendSavedResume(storage, "recovery", resume, async () => {
    storage.removeItem = () => { throw new Error("storage denied"); }; return receipt;
  });
  assert.deepEqual(confirmed, { receipt, cleared: false });
  const other = new MemoryStorage();
  const newer = { ...resume, requestId: "0199a001-0000-7000-8000-000000000005" };
  await sendSavedResume(other, "recovery", resume, async () => {
    other.removeItem("recovery"); rememberResume(other, "recovery", newer); return receipt;
  });
  assert.deepEqual(readSavedResume(other, "recovery", resume.computerId, resume.taskId), newer);
});
