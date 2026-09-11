import assert from "node:assert/strict";
import test from "node:test";
import { parseComputer, parseConsoleBootstrap } from "./generatedContracts.ts";

const id = "01994bed-e0d0-7000-8000-000000000001";
test("named Computer grants require an application binding and bounded closed permissions", () => {
  const grant = {
    computerId: id,
    requestId: id,
    principalId: "https://test#agent",
    oauthClientId: "agent",
    name: "Build work",
    permissions: ["read", "execute"],
    executionLimits: { maximumSeconds: 30, maximumOutputBytes: 1024, onInterruption: "stop_computer" },
    expiresAt: "2026-09-10T11:00:00Z",
  };
  assert.equal(parseComputer("issue_automation_grant", grant).oauthClientId, "agent");
  for (const altered of [
    { ...grant, owner: "forged" },
    { ...grant, oauthClientId: undefined },
    { ...grant, oauthClientId: "" },
    { ...grant, permissions: [] },
    { ...grant, executionLimits: { maximumSeconds: 30, maximumOutputBytes: 1024 } },
    { ...grant, executionLimits: { ...grant.executionLimits, onInterruption: "continue" } },
    { ...grant, permissions: ["admin"] },
    { ...grant, executionLimits: { maximumSeconds: 0, maximumOutputBytes: 1024 } },
    { ...grant, expiresAt: "tomorrow" },
  ]) {
    assert.throws(() => parseComputer("issue_automation_grant", altered));
  }
});
test("canonical Computer schemas reject unknown properties, invalid UUIDs, enums and missing flags", () => {
  const computer = {
    computerId: id,
    templateId: "default",
    phase: "ready",
    busy: false,
    canCreate: false,
    canStart: false,
    canStop: true,
    canDelete: false,
    canConnect: true,
    activeTaskId: null,
    activeExecution: null,
    canTransferFiles: false,
    createdAt: "2026-09-10T11:00:00Z",
    updatedAt: "2026-09-10T11:00:00Z",
  };
  assert.equal(parseComputer("computer", computer).computerId, id);
  for (const altered of [
    { ...computer, owner: "forged" },
    { ...computer, computerId: "bad" },
    { ...computer, phase: "unknown" },
    { ...computer, canConnect: undefined },
    { ...computer, createdAt: "yesterday" },
  ]) {
    assert.throws(() => parseComputer("computer", altered));
  }
  assert.throws(() => parseComputer("start_input", { requestId: id, image: "forged" }));
});
test("canonical terminal controls enforce closed versions, dimensions, dates and integer sequences", () => {
  const attach = {
    type: "attach",
    version: 2,
    computerId: id,
    token: "secret",
    cols: 80,
    rows: 24,
  };
  assert.equal(parseComputer("terminal_attach", attach).cols, 80);
  for (const altered of [
    { ...attach, version: 1 },
    { ...attach, cols: 501 },
    { ...attach, rows: 0 },
    { ...attach, cols: 2.5 },
  ]) {
    assert.throws(() => parseComputer("terminal_attach", altered));
  }
  assert.equal(
    parseComputer("terminal_server_control", { type: "replay_complete" }).type,
    "replay_complete",
  );
  for (const value of [
    { type: "lease", sequence: 0, expiresAt: "2026-09-10T11:00:00Z" },
    { type: "lease", sequence: 1.5, expiresAt: "2026-09-10T11:00:00Z" },
    { type: "ready", version: 2, expiresAt: "bad" },
    { type: "replay_complete", grant: "forged" },
  ]) {
    assert.throws(() => parseComputer("terminal_server_control", value));
  }
});
test("session bootstrap requires current permission and rejects inventory on the session surface", () => {
  const bootstrap = {
    profile: "operator",
    canReadInstallation: false,
    installation: {
      name: "Veoveo",
      productLabel: "Workspace",
      version: "test",
      offlineMode: false,
      generatedAt: "2026-09-10T11:00:00Z",
    },
    session: {
      displayName: "Alice",
      principalId: "https://test#alice",
      actorId: "https://test#alice",
      tenantId: "test",
      tenantName: "Test",
      workContext: "work",
      workContextTitle: "Work",
      membership: "contributor",
      invocationMode: "direct",
      availableTenants: [{ id: "test", name: "Test" }],
    },
  };
  assert.equal(parseConsoleBootstrap(bootstrap).canReadInstallation, false);
  assert.throws(() => parseConsoleBootstrap({ ...bootstrap, canReadInstallation: undefined }));
  assert.throws(() => parseConsoleBootstrap({ ...bootstrap, principals: [] }));
});
