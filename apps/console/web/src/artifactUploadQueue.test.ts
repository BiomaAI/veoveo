import { configureBrowserApplication } from "./browserApp.ts";
import assert from "node:assert/strict";
import test from "node:test";

configureBrowserApplication("console");
import { browserSession } from "./csrf.ts";
import { UploadQueue } from "./uploads/queue.ts";
import { describe, invalidSelection, requestId, sessionSchema, receiptSchema, savedSchema, partSchema, policySchema, type Policy, type Receipt, type Session } from "./uploads/model.ts";

import timestampCases from "../testdata/artifact-timestamps.json" with { type: "json" };
import { ChronoTimestamp } from "./chronoTimestamp.ts";

const origin = "https://uploads.example";
const storageKey = `veoveo.uploads.v2:${JSON.stringify([origin, "tenant", "alice", "operations"])}`;
const policy: Policy = {
  actor: "alice", workContext: "operations", allowed: true, explanation: "Enabled", destinationName: "Operations", accessDescription: "Owned by you", availableBytes: 1024,
  policy: { maxObjectBytes: 1024, tenantQuotaBytes: 1024, maxActiveUploadsPerTenant: 8, partBytes: 8, maxPartBytes: 8, maxParts: 128, parallelParts: 2, maxInflightBytes: 16, inactivitySeconds: 60, lifetimeSeconds: 3600, partTimeoutSeconds: 30, allowedMimeTypes: ["application/octet-stream"] },
};
const digest = async (blob: Blob) => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", await blob.arrayBuffer())), (value) => value.toString(16).padStart(2, "0")).join("");

test("Markdown selection uses its canonical media type and still requires policy admission", () => {
  const admitted = structuredClone(policy);
  admitted.policy!.allowedMimeTypes.push("text/markdown");
  for (const name of ["notes.md", "NOTES.MD", "notes.markdown"]) {
    for (const type of ["", "text/plain", "application/octet-stream", "text/markdown"]) {
      const descriptor = describe(new File(["# Notes\n"], name, { type }));
      assert.equal(descriptor.mimeType, "text/markdown");
      assert.equal(invalidSelection(descriptor, admitted), undefined);
      assert.equal(invalidSelection(descriptor, policy), "This file type is not allowed here.");
    }
  }
});

class TestHashWorker {
  onmessage?: (event: { data: { id: number; sha: string } }) => void;
  onerror?: () => void;
  stopped = false;
  postMessage(job: { id: number; blob: Blob }) { void digest(job.blob).then((sha) => { if (!this.stopped) this.onmessage?.({ data: { id: job.id, sha } }); }); }
  terminate() { this.stopped = true; }
}

test("generated upload admission preserves positive counters and safe browser arithmetic", () => {
  const part = { partNumber: 1, byteLen: 0, sha256: "0".repeat(64) };
  assert.deepEqual(partSchema.parse(part), part);
  for (const altered of [
    { ...part, partNumber: 0 }, { ...part, partNumber: 10001 },
    { ...part, byteLen: Number.MAX_SAFE_INTEGER + 1 },
  ]) assert.equal(partSchema.safeParse(altered).success, false);
  const session = {
    uploadId: requestId(), state: "open", descriptor: { filename: "sample.bin", mimeType: "application/octet-stream", byteLen: 0 },
    layout: { partBytes: 8, maxParts: 128, maxTotalBytes: 1024, parallelParts: 2 },
    acceptedBytes: 0, acceptedPartCount: 0, parts: [],
    createdAt: new Date().toISOString(), expiresAt: new Date().toISOString(),
  };
  assert.equal(sessionSchema.safeParse(session).success, true);
  for (const field of ["partBytes", "maxParts", "maxTotalBytes", "parallelParts"])
    assert.equal(sessionSchema.safeParse({ ...session, layout: { ...session.layout, [field]: 0 } }).success, false);
  assert.equal(sessionSchema.safeParse({ ...session, acceptedBytes: Number.MAX_SAFE_INTEGER + 1 }).success, false);
  assert.equal(policySchema.safeParse({ ...policy, availableBytes: Number.MAX_SAFE_INTEGER + 1 }).success, false);
  assert.equal(policySchema.safeParse({ ...policy, availableBytes: null }).success, true);
  assert.equal(invalidSelection(session.descriptor, { ...policy, availableBytes: null }), undefined);
  assert.match(invalidSelection({ ...session.descriptor, byteLen: 1 }, { ...policy, availableBytes: 0 })!, /Only 0 B/);
});

async function fixture() {
  const memory = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: { getItem: (key: string) => memory.get(key) ?? null, setItem: (key: string, value: string) => memory.set(key, value) } });
  Object.defineProperty(globalThis, "location", { configurable: true, value: { origin } });
  Object.defineProperty(globalThis, "window", { configurable: true, value: new EventTarget() });
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { onLine: true } });
  Object.defineProperty(globalThis, "Worker", { configurable: true, value: TestHashWorker });
  browserSession.csrfToken = "ephemeral-only";
  const file = new File([new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])], "sample.bin", { lastModified: 10 });
  const key = requestId(), uploadId = requestId(), artifactId = requestId();
  const receipt: Receipt = { uploadId: uploadId, artifactId: artifactId, artifactUri: `artifact://${artifactId}`, filename: file.name, mimeType: "application/octet-stream", byteLen: file.size, createdAt: new Date().toISOString(), sha256: await digest(file) };
  const status: Session = {
    uploadId: uploadId, descriptor: { filename: file.name, mimeType: "application/octet-stream", byteLen: file.size },
    state: "open", layout: { partBytes: 8, maxParts: 128, maxTotalBytes: 1024, parallelParts: 2 },
    acceptedBytes: 8, acceptedPartCount: 1, parts: [{ partNumber: 1, byteLen: 8, sha256: await digest(file.slice(0, 8)) }],
    createdAt: new Date().toISOString(), expiresAt: new Date(Date.now() + 60000).toISOString(),
  };
  memory.set(storageKey, JSON.stringify([{ key, descriptor: status.descriptor, lastModified: file.lastModified, uploadId, accepted: 8 }]));
  const calls: { method: string; path: string }[] = [];
  let policyResponse = policy;
  let response: (method: string, path: string) => Response = () => Response.json(status);
  globalThis.fetch = async (input, init) => {
    const path = String(input), method = init?.method ?? "GET";
    calls.push({ method, path });
    return path.endsWith("/policy") ? Response.json(policyResponse) : response(method, path);
  };
  const receipts: Receipt[] = [];
  const queue = new UploadQueue("alice", "operations", "tenant", (receipt) => receipts.push(receipt));
  return { memory, file, key, uploadId, receipt, status, queue, calls, receipts, respond: (value: typeof response) => { response = value; }, respondPolicy: (value: Policy) => { policyResponse = value; } };
}

test("saved v2 upload rows refuse retired and unknown fields before restoration", async () => {
  const f = await fixture();
  f.queue.dispose();
  const required = { key: f.key, descriptor: f.status.descriptor, lastModified: f.file.lastModified,
    uploadId: f.uploadId, accepted: 8 };
  const current = { ...required, cancelRequested: true, admissionStarted: true, restartRequired: true };
  assert.deepEqual(savedSchema.parse([current]), [current]);
  f.memory.set(storageKey, JSON.stringify([current]));
  const admitted = new UploadQueue("alice", "operations", "tenant", () => assert.fail("restoration cannot settle a receipt"));
  try {
    assert.equal(admitted.snapshot().persistenceError, undefined);
    const entry = admitted.snapshot().entries[0];
    assert.equal(entry.key, current.key);
    assert.equal(entry.cancelRequested, true);
    assert.equal(entry.admissionStarted, true);
    assert.equal(entry.restartRequired, true);
    assert.deepEqual(f.calls, []);
  } finally { admitted.dispose(); }
  const refused: unknown[] = [{ ...current, unexpected: "closed row" }];
  for (const [retired, active] of [
    ["cancel_requested", "cancelRequested"], ["admission_started", "admissionStarted"], ["restart_required", "restartRequired"],
  ]) {
    refused.push({ ...required, [retired]: true }, { ...required, [active]: false, [retired]: true });
  }
  for (const row of refused) {
    assert.equal(savedSchema.safeParse([row]).success, false, "unknown or retired keys must refuse rather than disappear");
    f.memory.set(storageKey, JSON.stringify([row]));
    const queue = new UploadQueue("alice", "operations", "tenant", () => assert.fail("invalid restoration cannot settle a receipt"));
    try {
      assert.deepEqual(queue.snapshot().entries, []);
      assert.match(queue.snapshot().persistenceError!, /could not be restored/);
      assert.deepEqual(f.calls, []);
      assert.equal(f.memory.get(storageKey), JSON.stringify([row]));
    } finally { queue.dispose(); }
  }
});

async function until(check: () => boolean) {
  const deadline = Date.now() + 3000;
  while (!check()) { assert.ok(Date.now() < deadline, "queue did not settle"); await new Promise((resolve) => setTimeout(resolve, 5)); }
}

test("upload events reconcile only known sessions and require a server receipt", async () => {
  const f = await fixture();
  await f.queue.initialize();
  const calls = f.calls.length;
  f.queue.reconcile(requestId());
  assert.equal(f.calls.length, calls);
  f.respond(() => Response.json({ ...f.status, state: "completed", acceptedBytes: 16, receipt: f.receipt }));
  f.queue.reconcile(f.uploadId);
  await until(() => f.queue.snapshot().entries[0].phase === "Ready");
  assert.equal(f.queue.snapshot().entries[0].file, undefined);
  assert.deepEqual(f.receipts, [f.receipt]);
  f.queue.dispose();
});

test("quota refresh distinguishes unknown allowance from an authoritative zero", async () => {
  for (const available of [null, 0]) {
    const f = await fixture();
    try {
      await f.queue.initialize();
      f.respondPolicy({ ...policy, availableBytes: available });
      f.respond(() => Response.json({ code: "quota_exceeded", message: "Quota blocked" }, { status: 429 }));
      f.queue.reconcile(f.uploadId);
      await until(() => f.calls.filter((call) => call.path.endsWith("/policy")).length === 2
        && f.queue.snapshot().policy?.availableBytes === available);
      const entry = f.queue.snapshot().entries[0];
      assert.equal(entry.phase, "Needs attention");
      if (available === null) assert.equal(entry.message, "Quota blocked");
      else assert.match(entry.message!, /0 B of storage is currently available/);
    } finally { f.queue.dispose(); }
  }
});

test("expired uploads require a new admission identity after current policy discovery", async () => {
  const f = await fixture();
  f.respond(() => Response.json({ ...f.status, state: "expired" }));
  await f.queue.initialize();
  assert.equal(f.queue.snapshot().entries[0].restartRequired, true);
  const calls = f.calls.length;
  f.queue.resume(f.key);
  assert.equal(f.calls.length, calls);
  await f.queue.restart(f.key);
  const fresh = f.queue.snapshot().entries[0];
  assert.notEqual(fresh.key, f.key);
  assert.equal(fresh.uploadId, undefined);
  assert.equal(fresh.accepted, 0);
  assert.equal(fresh.phase, "Select file");
  assert.equal(f.calls.at(-1)?.path, "/console/api/artifact-uploads/policy");
  f.queue.dispose();
});

test("reselected wrong bytes preserve accepted progress and never reach PUT", async () => {
  const f = await fixture();
  Object.defineProperty(globalThis, "XMLHttpRequest", { configurable: true, value: class { constructor() { assert.fail("wrong file reached the transport"); } } });
  try {
    await f.queue.initialize();
    assert.equal(f.queue.snapshot().entries[0].phase, "Select file");
    f.queue.reselect(f.key, new File([new Uint8Array(16).fill(99)], "sample.bin", { lastModified: 10 }));
    await until(() => f.queue.snapshot().entries[0].phase === "Needs attention");
    const entry = f.queue.snapshot().entries[0];
    assert.equal(entry.accepted, 8); assert.equal(entry.uploadId, f.uploadId); assert.equal(entry.file, undefined);
    assert.match(entry.message!, /does not match/);
    assert.ok(f.calls.every((call) => call.method === "GET"));
  } finally { f.queue.dispose(); }
});

test("resume checks saved bytes and sends only the missing part, then waits for a receipt", async () => {
  const f = await fixture();
  const puts: string[] = [];
  let sealed = false;
  f.respond((method) => {
    if (method === "POST") { sealed = true; return Response.json({ ...f.status, state: "finalizing", acceptedBytes: 16, acceptedPartCount: 2 }, { status: 202 }); }
    return Response.json(sealed ? { ...f.status, state: "completed", acceptedBytes: 16, acceptedPartCount: 2, receipt: f.receipt } : f.status);
  });
  class PartRequest {
    upload: { onprogress?: (event: { loaded: number }) => void } = {};
    onload?: () => void; onabort?: () => void; onerror?: () => void; ontimeout?: () => void;
    status = 200; timeout = 0; responseText = ""; path = ""; headers = new Map<string, string>();
    open(method: string, path: string) { assert.equal(method, "PUT"); this.path = path; }
    setRequestHeader(key: string, value: string) { this.headers.set(key, value); }
    getResponseHeader() { return null; }
    abort() { this.onabort?.(); }
    send(blob: Blob) {
      puts.push(this.path);
      void digest(blob).then((sha) => {
        assert.equal(this.headers.get("x-veoveo-part-sha256"), sha);
        this.upload.onprogress?.({ loaded: blob.size });
        assert.notEqual(f.queue.snapshot().entries[0].phase, "Ready");
        this.responseText = JSON.stringify({ partNumber: 2, byteLen: blob.size, sha256: sha });
        this.onload?.();
      });
    }
  }
  Object.defineProperty(globalThis, "XMLHttpRequest", { configurable: true, value: PartRequest });
  try {
    await f.queue.initialize(); f.queue.reselect(f.key, f.file);
    await until(() => f.queue.snapshot().entries[0].phase === "Ready");
    assert.deepEqual(puts, [`/console/api/artifact-uploads/${f.uploadId}/parts/2`]);
    assert.deepEqual(f.receipts, [f.receipt]);
    assert.equal(f.queue.snapshot().entries[0].file, undefined);
    const persisted = f.memory.get(storageKey)!;
    assert.ok(!persisted.includes("ephemeral-only"));
    assert.ok(!persisted.includes('"file"')); assert.ok(!persisted.includes('"parts"'));
  } finally { f.queue.dispose(); }
});

test("a lost part response checks short-request authentication before sending bytes again", async () => {
  const f = await fixture();
  let failedPart = false;
  let puts = 0;
  f.respond(() => failedPart ? Response.json({ message: "Sign in required" }, { status: 401 }) : Response.json(f.status));
  class LostResponse {
    upload = {}; status = 502; timeout = 0; responseText = "";
    onload?: () => void; onabort?: () => void;
    open() {} setRequestHeader() {} getResponseHeader() { return null; }
    abort() { this.onabort?.(); }
    send() { puts += 1; failedPart = true; queueMicrotask(() => this.onload?.()); }
  }
  Object.defineProperty(globalThis, "XMLHttpRequest", { configurable: true, value: LostResponse });
  try {
    await f.queue.initialize(); f.queue.reselect(f.key, f.file);
    await until(() => f.queue.snapshot().entries[0].phase === "Sign in to continue");
    assert.equal(puts, 1);
    assert.equal(f.queue.snapshot().entries[0].accepted, 8);
    assert.equal(f.queue.snapshot().entries[0].file, undefined);
  } finally { f.queue.dispose(); }
});

test("completion wins cancellation without losing the receipt or deleting an artifact", async () => {
  const f = await fixture();
  let cancelled = false;
  f.respond((method) => {
    if (method === "DELETE") { cancelled = true; return Response.json({ message: "Completed" }, { status: 409 }); }
    return Response.json(cancelled ? { ...f.status, state: "completed", receipt: f.receipt } : f.status);
  });
  try {
    await f.queue.initialize(); await f.queue.cancel(f.key);
    assert.equal(f.queue.snapshot().entries[0].phase, "Ready");
    assert.deepEqual(f.receipts, [f.receipt]);
    assert.equal(f.calls.filter((call) => call.method === "DELETE").length, 1);
    f.queue.clearCompleted(); assert.equal(f.queue.snapshot().entries.length, 0);
    assert.equal(f.calls.filter((call) => call.method === "DELETE").length, 1);
  } finally { f.queue.dispose(); }
});

test("reload recovers a verified receipt without asking for file access, and scopes stay isolated", async () => {
  const f = await fixture();
  f.respond(() => Response.json({ ...f.status, state: "completed", receipt: f.receipt }));
  const foreign = new UploadQueue("bob", "operations", "tenant", () => assert.fail("foreign receipt"));
  try {
    assert.equal(foreign.snapshot().entries.length, 0);
    await f.queue.initialize();
    assert.equal(f.queue.snapshot().entries[0].phase, "Ready");
    assert.equal(f.queue.snapshot().entries[0].file, undefined);
    assert.equal(f.receipts.length, 1);
  } finally { f.queue.dispose(); foreign.dispose(); }
});


test("upload timestamp admission preserves all current Rust-produced receipt bytes", async () => {
  assert.equal(timestampCases.length, 12);
  const f = await fixture();
  try {
    for (const row of timestampCases) {
      assert.deepEqual(receiptSchema.parse(row.receipt), row.receipt, row.name);
      assert.equal(ChronoTimestamp.parse(row.receipt.createdAt).wire, row.receipt.createdAt);
      for (const receipt of [undefined, null, row.receipt]) {
        const session = { ...f.status, createdAt: row.receipt.createdAt, expiresAt: row.receipt.createdAt, ...(receipt === undefined ? {} : { receipt }) };
        assert.deepEqual(sessionSchema.parse(session), session, row.name);
      }
    }
    const a = timestampCases.find((row) => row.name === "adjacent_nanosecond_a")!;
    const b = timestampCases.find((row) => row.name === "adjacent_nanosecond_b")!;
    assert.equal(ChronoTimestamp.parse(a.receipt.createdAt).compare(ChronoTimestamp.parse(b.receipt.createdAt)), -1);
  } finally { f.queue.dispose(); }
});

const malformedTimestamps = ["2026-02-30T12:34:56Z", "+262143-01-01T00:00:00Z", "+262142-12-31T23:59:59-00:01"];

test("upload calendar and UTC-range refusal precedes receipt, transfer and cache effects", async () => {
  for (const wire of malformedTimestamps) {
    for (const field of ["createdAt", "expiresAt", "receipt"] as const) {
      const f = await fixture();
      Object.defineProperty(globalThis, "XMLHttpRequest", { configurable: true, value: class {
        constructor() { assert.fail("refused timestamp reached transfer"); }
      } });
      try {
        const status = { ...f.status, state: "completed", receipt: f.receipt };
        if (field === "receipt") status.receipt = { ...f.receipt, createdAt: wire };
        else status[field] = wire;
        assert.equal(sessionSchema.safeParse(status).success, false);
        f.respond(() => Response.json(status));
        await f.queue.initialize();
        assert.notEqual(f.queue.snapshot().entries[0].phase, "Ready");
        assert.equal(f.queue.snapshot().entries[0].receipt, undefined);
        assert.deepEqual(f.receipts, []);
        assert.ok(f.calls.every((call) => call.method === "GET"));
        assert.ok(JSON.parse(f.memory.get(storageKey)!).every((entry: { receipt?: Receipt }) => entry.receipt === undefined));
      } finally { f.queue.dispose(); }
    }
    const f = await fixture();
    f.queue.dispose();
    const saved = [{ key: f.key, descriptor: f.status.descriptor, lastModified: f.file.lastModified,
      uploadId: f.uploadId, accepted: 8, receipt: { ...f.receipt, createdAt: wire } }];
    assert.equal(receiptSchema.safeParse(saved[0].receipt).success, false);
    assert.equal(savedSchema.safeParse(saved).success, false);
    f.memory.set(storageKey, JSON.stringify(saved));
    const queue = new UploadQueue("alice", "operations", "tenant", (receipt) => f.receipts.push(receipt));
    try {
      assert.deepEqual(queue.snapshot().entries, []);
      assert.match(queue.snapshot().persistenceError!, /could not be restored/);
      assert.deepEqual(f.receipts, []);
      assert.deepEqual(f.calls, []);
      assert.equal(f.memory.get(storageKey), JSON.stringify(saved));
    } finally { queue.dispose(); }
  }
});


test("open upload resume refuses malformed timestamps before hashing or sending saved-file bytes", async () => {
  for (const wire of malformedTimestamps) {
    for (const field of ["createdAt", "expiresAt"] as const) {
      const f = await fixture();
      try {
        // Admit the existing open session, then exercise the same original-file
        // reselect workflow as the valid missing-part resume control above.
        await f.queue.initialize();
        assert.equal(f.queue.snapshot().entries[0].phase, "Select file");
        const saved = JSON.parse(f.memory.get(storageKey)!)[0];
        const reads = f.calls.filter((call) => call.path.endsWith(`/${f.uploadId}`)).length;
        let hashWorkers = 0, hashJobs = 0, partRequests = 0, bodySends = 0;
        class ObservedHashWorker extends TestHashWorker {
          constructor() { super(); hashWorkers += 1; }
          postMessage(job: { id: number; blob: Blob }) { hashJobs += 1; super.postMessage(job); }
        }
        class ObservedPartRequest {
          upload = {}; status = 200; timeout = 0; responseText = "";
          constructor() { partRequests += 1; }
          open() {} setRequestHeader() {} getResponseHeader() { return null; }
          abort() {}
          send() { bodySends += 1; throw new Error("refused open session reached a part body"); }
        }
        Object.defineProperty(globalThis, "Worker", { configurable: true, value: ObservedHashWorker });
        Object.defineProperty(globalThis, "XMLHttpRequest", { configurable: true, value: ObservedPartRequest });
        f.respond(() => Response.json({ ...f.status, [field]: wire }));
        f.queue.reselect(f.key, f.file);
        await until(() => f.calls.filter((call) => call.path.endsWith(`/${f.uploadId}`)).length > reads
          && f.queue.snapshot().entries[0].phase === "Needs attention");
        assert.equal(hashWorkers, 0, `${field}: no Hashing allocation`);
        assert.equal(hashJobs, 0, `${field}: no file hashing`);
        assert.equal(partRequests, 0, `${field}: no part transport`);
        assert.equal(bodySends, 0, `${field}: no body send`);
        assert.ok(f.calls.every((call) => call.method === "GET"), `${field}: no completion request`);
        const entry = f.queue.snapshot().entries[0];
        assert.equal(entry.uploadId, f.uploadId);
        assert.equal(entry.accepted, saved.accepted);
        assert.equal(entry.sent, saved.accepted);
        assert.deepEqual(entry.descriptor, saved.descriptor);
        assert.equal(entry.receipt, undefined);
        assert.deepEqual(f.receipts, []);
        const current = JSON.parse(f.memory.get(storageKey)!)[0];
        assert.equal(current.uploadId, saved.uploadId);
        assert.equal(current.accepted, saved.accepted);
        assert.deepEqual(current.descriptor, saved.descriptor);
        assert.equal(current.receipt, undefined);
      } finally { f.queue.dispose(); }
    }
  }
});
