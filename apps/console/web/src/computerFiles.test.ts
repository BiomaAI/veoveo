import assert from "node:assert/strict";
import test from "node:test";
import { artifactId, fileFinished, readSavedFile, rememberFile, retainedPath, sendSavedFile } from "./computers/fileRequest.ts";
import type { FileTransferView, TransferFileInput } from "./generated/computers.ts";

const input: TransferFileInput = {
  computerId: "0199a001-0000-7000-8000-000000000001", requestId: "0199a001-0000-7000-8000-000000000002", grantId: null,
  transfer: { kind: "import", artifactId: "0199a001-0000-7000-8000-000000000003", path: "project/雪.bin" },
  limits: { maximumBytes: 1024, maximumSeconds: 30, onInterruption: "stop_computer" },
};
const receipt: FileTransferView = { computerId: input.computerId, taskId: "0199a001-0000-7000-8000-000000000004",
  direction: "import", stage: "queued", message: "Preparing transfer", cancellationRequestedAt: null, canCancel: true, result: null,
  createdAt: "2026-09-11T00:00:00Z", updatedAt: "2026-09-11T00:00:00Z", completedAt: null };
class MemoryStorage {
  values = new Map<string, string>();
  getItem(key: string) { return this.values.get(key) ?? null; }
  setItem(key: string, value: string) { this.values.set(key, value); }
  removeItem(key: string) { this.values.delete(key); }
}
test("a lost file reply preserves the exact Artifact, path and bounded request", async () => {
  const storage = new MemoryStorage();
  await assert.rejects(sendSavedFile(storage, "scope", input, async sent => {
    assert.deepEqual(readSavedFile(storage, "scope", input.computerId)?.input, sent);
    throw new Error("reply lost");
  }));
  const restored = readSavedFile(storage, "scope", input.computerId)!;
  assert.deepEqual(restored.input, input);
  for (const changed of [
    { ...input, transfer: { ...input.transfer, path: "another.bin" } },
    { ...input, limits: { ...input.limits, maximumBytes: 2048 } },
    { ...input, transfer: { kind: "import" as const, path: input.transfer.path, artifactId: receipt.taskId } },
  ]) {
    await assert.rejects(sendSavedFile(storage, "scope", changed, async () => { assert.fail("changed intent reached the service"); }));
  }
  assert.deepEqual(await sendSavedFile(storage, "scope", restored.input, async () => receipt), { receipt, saved: true });
});
test("file request storage and result identities prevent unintended dispatch or replacement", async () => {
  const storage = new MemoryStorage();
  storage.setItem = () => { throw new Error("quota exceeded"); };
  await assert.rejects(sendSavedFile(storage, "scope", input, async () => { assert.fail("request was not saved"); }));
  const other = new MemoryStorage();
  rememberFile(other, "scope", input);
  assert.throws(() => readSavedFile(other, "scope", receipt.taskId));
  await assert.rejects(sendSavedFile(other, "scope", input, async () => ({ ...receipt, computerId: receipt.taskId })));
  await assert.rejects(sendSavedFile(other, "scope", input, async () => ({ ...receipt, direction: "export" })));
  await sendSavedFile(other, "scope", input, async () => receipt);
  await assert.rejects(sendSavedFile(other, "scope", input, async () => ({ ...receipt, taskId: input.requestId })));
});
test("a confirmed file receipt survives storage failure and cannot restore dismissed intent", async () => {
  for (const failure of ["read", "write", "dismiss"]) {
    const storage = new MemoryStorage();
    const reply = await sendSavedFile(storage, "scope", input, async () => {
      if (failure === "read") storage.getItem = () => { throw new Error("storage blocked"); };
      if (failure === "write") storage.setItem = () => { throw new Error("storage full"); };
      if (failure === "dismiss") storage.removeItem("scope");
      return receipt;
    });
    assert.deepEqual(reply, { receipt, saved: false });
    if (failure === "dismiss") assert.equal(storage.getItem("scope"), null);
  }
});
test("file input keeps canonical paths and Artifact addresses without query authority", () => {
  assert.equal(retainedPath("project/雪 archive.tar"), "project/雪 archive.tar");
  for (const path of ["", "/etc/passwd", "a//b", "a/../b", "a/./b", "a/", "a\nb", "雪".repeat(342)])
    assert.throws(() => retainedPath(path));
  const id = "0199a001-0000-7000-8000-000000000003";
  assert.equal(artifactId(`artifact://${id}`), id);
  assert.equal(artifactId(id), id);
  for (const value of [`artifact://${id}?token=private`, `artifact://${id}/`, id.toUpperCase(), "00000000-0000-4000-8000-000000000001"])
    assert.throws(() => artifactId(value));
  assert.equal(fileFinished({ ...receipt, stage: "completed" }), false);
  assert.equal(fileFinished({ ...receipt, stage: "failed" }), true);
  assert.equal(fileFinished({ ...receipt, stage: "recovery_required" }), false);
});
