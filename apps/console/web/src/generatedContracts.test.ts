import assert from "node:assert/strict";
import test from "node:test";
import { parseAudit, parseComputer, parseConsoleBootstrap } from "./generatedContracts.ts";
import auditSchema from "./generated/audit.schema.json" with { type: "json" };
import computerSchema from "./generated/computers.schema.json" with { type: "json" };
import agentSchema from "./generated/agent-management.schema.json" with { type: "json" };
import { compileGeneratedSchema } from "./jsonSchema.ts";

const id = "01994bed-e0d0-7000-8000-000000000001";
test("interactive access and pairing identities use canonical UUIDv7 admission", () => {
  const revoke = { computerId: id, grantId: id };
  const pairing = { computerId: id, pairingId: id, expiresAt: "2026-10-02T11:00:00Z" };
  assert.deepEqual(parseComputer("revoke_access_input", revoke), revoke);
  assert.deepEqual(parseComputer("cli_pairing_challenge", pairing), pairing);
  for (const invalid of [
    id.toUpperCase(), id.replaceAll("-", ""), `${id}\n`,
    id.replace("-7000-", "-4000-"), id.replace("-8000-", "-c000-"),
    "00000000-0000-0000-0000-000000000000",
  ]) {
    assert.throws(() => parseComputer("revoke_access_input", { ...revoke, grantId: invalid }));
    assert.throws(() => parseComputer("cli_pairing_challenge", { ...pairing, pairingId: invalid }));
  }
  // Browser-generated idempotency keys retain their separate UUID profile.
  const request = { requestId: id.replace("-7000-", "-4000-") };
  assert.deepEqual(parseComputer("create_input", request), request);
});
test("every generated contract definition compiles with the browser's no-eval validator", () => {
  for (const schema of [auditSchema, computerSchema, agentSchema]) {
    for (const name of Object.keys(schema.$defs)) {
      assert.doesNotThrow(() => compileGeneratedSchema({
        $schema: schema.$schema, $defs: schema.$defs, $ref: `#/$defs/${name}`,
      }), name);
    }
  }
});
test("audit pages and filters enforce nonzero trace IDs and preserve valid records", () => {
  const record = {
    id, requestId: id, traceId: "00000000000000000000000000000001",
    occurredAt: "2026-09-30T11:00:00Z", class: "api_activity",
    detail: { kind: "read", method: "resource_read" },
    outcome: "allowed", reason: "accepted", target: { kind: "installation" },
  };
  const page = { records: [record], next: null };
  assert.deepEqual(parseAudit("page", page), page);
  const query = { partition: { kind: "installation" }, order: "newest_first", limit: 50 };
  assert.deepEqual(parseAudit("query", { ...query, trace: record.traceId }), { ...query, trace: record.traceId });
  for (const trace of ["0".repeat(32), "A".repeat(32), "a".repeat(31), "z".repeat(32)]) {
    assert.throws(() => parseAudit("page", { records: [{ ...record, traceId: trace }] }));
    assert.throws(() => parseAudit("query", { ...query, trace }));
  }
  for (const changed of [
    { ...record, requestId: id.replace("-7000-", "-4000-") },
    { ...record, detail: { kind: "read", method: "undeclared" } },
    { ...record, occurredAt: "yesterday" },
    { ...record, callerPayload: "undeclared" },
  ]) assert.throws(() => parseAudit("page", { records: [changed] }));
});
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
    accessMode: "owner",
    grantedAccess: [],
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
    { ...computer, computerId: id.toUpperCase() },
    { ...computer, computerId: id.replaceAll("-", "") },
    { ...computer, computerId: id.replace("-7000-", "-4000-") },
    { ...computer, computerId: id.replace("-8000-", "-c000-") },
    { ...computer, computerId: `${id}\n` },
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
    canReadAudit: false,
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
  assert.throws(() => parseConsoleBootstrap({ ...bootstrap, canReadAudit: undefined }));
  assert.throws(() => parseConsoleBootstrap({ ...bootstrap, principals: [] }));
});
