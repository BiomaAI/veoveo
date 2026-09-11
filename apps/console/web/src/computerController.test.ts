import assert from "node:assert/strict";
import test from "node:test";
import { ComputersController, type ComputerPorts } from "./computers/controller.ts";
import type { ComputerSnapshot, ComputerView, OperationReceipt } from "./generated/computers.ts";
import type { LiveState } from "./computers/events.ts";

const computerId = "01994bed-e0d0-7000-8000-000000000001";
const snapshot = (canCreate = true): ComputerSnapshot => ({
  availability: "available",
  canCreate,
  computers: [],
  nextCursor: null,
  template: null,
  limits: null,
});
const settled = async () => {
  await new Promise((resolve) => setImmediate(resolve));
};
function grantedComputer(grantId: string): ComputerView {
  return {
    computerId, accessMode: "granted", templateId: "development", phase: "ready", busy: false,
    canCreate: false, canStart: false, canStop: true, canDelete: false, canConnect: false,
    canTransferFiles: false, activeExecution: null, activeTaskId: null,
    createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(),
    grantedAccess: [{ grantId, name: "Lifecycle", permissions: ["read", "stop"], executionLimits: null,
      canTransferFiles: false, expiresAt: new Date(Date.now() + 3600000).toISOString() }],
  };
}
function fixture() {
  const saved = new Map<string, string>();
  const calls: string[] = [];
  const operations = new Map<string, OperationReceipt>();
  let change = () => {};
  let live: (state: LiveState) => void = () => {};
  const ports: ComputerPorts = {
    storage: {
      getItem: (key) => saved.get(key) ?? null,
      setItem: (key, value) => {
        saved.set(key, value);
      },
    },
    read: async () => snapshot(),
    operation: async (_computer, id) => {
      const receipt = operations.get(id);
      if (!receipt) throw new Error("Unknown fixture operation");
      return receipt;
    },
    command: async (action, requestId) => {
      calls.push(requestId);
      const receipt: OperationReceipt = { action, computerId, taskId: requestId, status: "queued" };
      operations.set(requestId, receipt);
      return receipt;
    },
    watch: (changed, status) => {
      change = changed;
      live = status;
      status("live");
      return () => {};
    },
  };
  return { ports, saved, calls, operations, change: () => change(), live: (state: LiveState) => live(state) };
}
test("live operation reads settle saved receipts without redispatching commands", async () => {
  const f = fixture();
  const controller = new ComputersController("alice", f.ports);
  controller.start();
  await settled();
  await controller.command("create");
  await settled();
  const intent = controller.snapshot().intents[0];
  const receipt = intent.receipt!;
  f.operations.set(receipt.taskId, {...receipt, status: "running"});
  f.change();
  await settled();
  assert.equal(controller.snapshot().intents[0].receipt?.status, "running");
  f.operations.set(receipt.taskId, {...receipt, status: "succeeded"});
  await controller.retry(intent.requestId);
  assert.equal(controller.snapshot().intents[0].receipt?.status, "succeeded");
  assert.equal(f.calls.length, 1);
  controller.dispose();
  const reloaded = new ComputersController("alice", f.ports);
  assert.equal(reloaded.snapshot().intents[0].receipt?.status, "succeeded");
  reloaded.dispose();
});
test("Computer reload retries the exact saved lifecycle identity after an ambiguous response", async () => {
  const f = fixture();
  let fail = true;
  const original = f.ports.command;
  f.ports.command = async (...args) => {
    const receipt = await original(...args);
    if (fail) throw new Error("response lost after acceptance");
    return receipt;
  };
  const first = new ComputersController("alice", f.ports);
  first.start();
  await settled();
  await first.command("create");
  const intent = first.snapshot().intents[0];
  assert.ok(intent.error);
  assert.equal(intent.receipt, undefined);
  first.dispose();
  fail = false;
  const reloaded = new ComputersController("alice", f.ports);
  reloaded.start();
  await settled();
  await reloaded.retry(intent.requestId);
  assert.deepEqual(f.calls, [intent.requestId, intent.requestId]);
  assert.equal(reloaded.snapshot().intents[0].receipt?.computerId, computerId);
  assert.equal(new ComputersController("bob", f.ports).snapshot().intents.length, 0);
  assert.ok([...f.saved.values()].every((raw) => !/token|cookie|commandText/i.test(raw)));
  reloaded.dispose();
});
test("Computer invalidations during a read trigger one current reread and report lost live authority", async () => {
  const f = fixture();
  let finish!: (value: ComputerSnapshot) => void;
  let reads = 0;
  f.ports.read = async () => {
    reads++;
    return reads === 1
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : snapshot(false);
  };
  const controller = new ComputersController("alice", f.ports);
  controller.start();
  f.change();
  f.change();
  f.change();
  finish(snapshot());
  await settled();
  assert.equal(reads, 2);
  assert.equal(controller.snapshot().snapshot?.canCreate, false);
  assert.equal(controller.snapshot().stale, false);
  f.live("reconnecting");
  assert.equal(controller.snapshot().stale, true);
  controller.dispose();
});
test("Computer late reads from a disposed epoch cannot replace a restarted controller", async () => {
  const f = fixture();
  let finish!: (value: ComputerSnapshot) => void;
  let reads = 0;
  f.ports.read = async () =>
    ++reads === 1
      ? new Promise((resolve) => {
          finish = resolve;
        })
      : snapshot(false);
  const controller = new ComputersController("alice", f.ports);
  controller.start();
  controller.dispose();
  controller.start();
  await settled();
  finish(snapshot(true));
  await settled();
  assert.equal(controller.snapshot().snapshot?.canCreate, false);
  controller.dispose();
});
test("Computer lifecycle is persisted before dispatch and concurrent retries share one in-flight request", async () => {
  const f = fixture();
  let finish!: (value: OperationReceipt) => void;
  f.ports.command = async (action, requestId) => {
    assert.equal(action, "create");
    assert.ok([...f.saved.values()].some((value) => value.includes(requestId)));
    f.calls.push(requestId);
    return new Promise((resolve) => {
      finish = resolve;
    });
  };
  const controller = new ComputersController("alice", f.ports);
  const pending = controller.command("create");
  const id = controller.snapshot().intents[0].requestId;
  await controller.retry(id);
  assert.equal(f.calls.length, 1);
  finish({ action: "create", computerId, taskId: id, status: "queued" });
  await pending;
  controller.dispose();
});
test("Computer recovery storage failure prevents a fresh mutation from reaching the service", async () => {
  const f = fixture();
  f.ports.storage.setItem = () => {
    throw new Error("quota");
  };
  const controller = new ComputersController("alice", f.ports);
  await controller.command("create");
  assert.equal(f.calls.length, 0);
  assert.ok(controller.snapshot().persistenceError);
  controller.dispose();
});

test("Computer replaced subscriptions cannot deliver stale status or trigger reads", async () => {
  const f = fixture();
  const observers: Array<{ changed: () => void; status: (state: LiveState) => void }> = [];
  let reads = 0;
  f.ports.read = async () => {
    reads++;
    return snapshot();
  };
  f.ports.watch = (changed, status) => {
    observers.push({ changed, status });
    status("live");
    return () => {};
  };
  const controller = new ComputersController("alice", f.ports);
  controller.start();
  await settled();
  controller.reconnect();
  await settled();
  const before = reads;
  observers[0].status("denied");
  observers[0].changed();
  await settled();
  assert.equal(controller.snapshot().live, "live");
  assert.equal(reads, before);
  controller.dispose();
});

test("granted lifecycle keeps its original grant across a lost reply, reload and changed access", async () => {
  const f = fixture();
  const originalGrant = "01994bed-e0d0-7000-8000-000000000002";
  const replacementGrant = "01994bed-e0d0-7000-8000-000000000003";
  let computer = grantedComputer(originalGrant);
  f.ports.read = async () => ({ ...snapshot(), computers: [computer] });
  const grants: Array<string | undefined> = [];
  f.ports.command = async (action, requestId, selected, _signal, grant) => {
    assert.equal(selected, computerId);
    grants.push(grant);
    f.calls.push(requestId);
    if (grants.length === 1) throw new Error("reply lost");
    return { action, computerId, taskId: requestId, status: "queued" };
  };
  const first = new ComputersController("agent", f.ports);
  first.start();
  await settled();
  await first.command("stop", computerId);
  const intent = first.snapshot().intents[0];
  assert.equal(intent.grantId, originalGrant);
  first.dispose();
  computer = grantedComputer(replacementGrant);
  const restored = new ComputersController("agent", f.ports);
  restored.start();
  await settled();
  await restored.command("stop", computerId);
  assert.deepEqual(grants, [originalGrant, originalGrant]);
  assert.deepEqual(f.calls, [intent.requestId, intent.requestId]);
  await restored.command("stop", computerId);
  assert.equal(grants.at(-1), replacementGrant);
  restored.dispose();
});

test("a granted view without a current matching permission never sends an owner lifecycle request", async () => {
  const f = fixture();
  const computer = grantedComputer("01994bed-e0d0-7000-8000-000000000002");
  computer.grantedAccess[0].permissions = ["read"];
  f.ports.read = async () => ({ ...snapshot(), computers: [computer] });
  const controller = new ComputersController("agent", f.ports);
  controller.start();
  await settled();
  await controller.command("stop", computerId);
  assert.equal(f.calls.length, 0);
  assert.equal(controller.snapshot().intents.length, 0);
  assert.ok(controller.snapshot().error);
  controller.dispose();
});
