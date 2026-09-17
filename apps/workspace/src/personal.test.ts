import { test } from "node:test";
import assert from "node:assert/strict";
import { emptyPersonal, observePersonal, personalAttention } from "./personal.ts";

test("personal Task baselines, replay and revoked reads do not create misleading attention", () => {
  let state = observePersonal(emptyPersonal(), { kind: "inventory", invitations: 0, limited: false, operations: [{ id: "a", phase: "task", revision: 1 }] });
  state = observePersonal(state, { kind: "task", operation: "a", state: "completed", updatedAt: "initial" });
  assert.equal(state.unread.size, 0, "old completion is not new activity");
  state = observePersonal(state, { kind: "inventory", invitations: 0, limited: false, operations: [{ id: "a", phase: "task", revision: 1 }, { id: "b", phase: "task", revision: 1 }] });
  state = observePersonal(state, { kind: "task", operation: "b", state: "input_required", updatedAt: "one" });
  assert.equal(personalAttention(state), 1);
  state = observePersonal(state, { kind: "task", operation: "b", state: "completed", updatedAt: "two" });
  assert.equal(personalAttention(state), 0);
  assert.equal(state.unread.size, 1);
  state = { ...state, unread: new Set() };
  state = observePersonal(state, { kind: "task", operation: "b", state: "completed", updatedAt: "two" });
  assert.equal(state.unread.size, 0, "reconnect cannot replay a dismissed alert");
  state = observePersonal(state, { kind: "task_unavailable", operation: "b" });
  assert.equal(state.items.get("b")?.task, undefined);
  state = observePersonal(state, { kind: "inventory", invitations: 0, limited: false, operations: [] });
  assert.equal(state.items.size, 0, "removed private observations leave no retained badge");
});

test("new synchronous completion and native input use the same private attention model", () => {
  let state = observePersonal(emptyPersonal(), { kind: "inventory", invitations: 0, limited: false, operations: [] });
  state = observePersonal(state, { kind: "inventory", invitations: 1, limited: true, operations: [{ id: "new", phase: "completed", revision: 1 }, { id: "input", phase: "input_required", revision: 1 }] });
  assert.deepEqual([...state.unread], ["new"]);
  assert.equal(personalAttention(state), 1);
  assert.equal(state.limited, true);
});
