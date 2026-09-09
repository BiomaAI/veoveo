import assert from "node:assert/strict";
import test from "node:test";
import { consoleSession } from "./csrf.ts";
import { UploadQueue } from "./uploads/queue.ts";
import { requestId, type Policy, type Receipt, type Session } from "./uploads/model.ts";

const origin = "https://uploads.example";
const storageKey = `veoveo.uploads.v1:${JSON.stringify([origin, "tenant", "alice", "operations"])}`;
const policy: Policy = {
  actor: "alice", work_context: "operations", allowed: true, explanation: "Enabled", destination_name: "Operations", access_description: "Owned by you", available_bytes: 1024,
  policy: { max_object_bytes: 1024, tenant_quota_bytes: 1024, max_active_uploads_per_tenant: 8, part_bytes: 8, max_part_bytes: 8, max_parts: 128, parallel_parts: 2, max_inflight_bytes: 16, inactivity_seconds: 60, lifetime_seconds: 3600, part_timeout_seconds: 30, allowed_mime_types: ["application/octet-stream"] },
};
const digest = async (blob: Blob) => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", await blob.arrayBuffer())), (value) => value.toString(16).padStart(2, "0")).join("");

class TestHashWorker {
  onmessage?: (event: { data: { id: number; sha: string } }) => void;
  onerror?: () => void;
  stopped = false;
  postMessage(job: { id: number; blob: Blob }) { void digest(job.blob).then((sha) => { if (!this.stopped) this.onmessage?.({ data: { id: job.id, sha } }); }); }
  terminate() { this.stopped = true; }
}

async function fixture() {
  const memory = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: { getItem: (key: string) => memory.get(key) ?? null, setItem: (key: string, value: string) => memory.set(key, value) } });
  Object.defineProperty(globalThis, "location", { configurable: true, value: { origin } });
  Object.defineProperty(globalThis, "window", { configurable: true, value: new EventTarget() });
  Object.defineProperty(globalThis, "navigator", { configurable: true, value: { onLine: true } });
  Object.defineProperty(globalThis, "Worker", { configurable: true, value: TestHashWorker });
  consoleSession.csrfToken = "ephemeral-only";
  const file = new File([new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])], "sample.bin", { lastModified: 10 });
  const key = requestId(), uploadId = requestId(), artifactId = requestId();
  const receipt: Receipt = { upload_id: uploadId, artifact_id: artifactId, artifact_uri: `artifact://${artifactId}`, filename: file.name, mime_type: "application/octet-stream", byte_len: file.size, created_at: new Date().toISOString(), sha256: await digest(file) };
  const status: Session = {
    upload_id: uploadId, descriptor: { filename: file.name, mime_type: "application/octet-stream", byte_len: file.size },
    state: "open", layout: { part_bytes: 8, max_parts: 128, max_total_bytes: 1024, parallel_parts: 2 },
    accepted_bytes: 8, accepted_part_count: 1, parts: [{ part_number: 1, byte_len: 8, sha256: await digest(file.slice(0, 8)) }],
    created_at: new Date().toISOString(), expires_at: new Date(Date.now() + 60000).toISOString(),
  };
  memory.set(storageKey, JSON.stringify([{ key, descriptor: status.descriptor, lastModified: file.lastModified, uploadId, accepted: 8 }]));
  const calls: { method: string; path: string }[] = [];
  let response: (method: string, path: string) => Response = () => Response.json(status);
  globalThis.fetch = async (input, init) => {
    const path = String(input), method = init?.method ?? "GET";
    calls.push({ method, path });
    return path.endsWith("/policy") ? Response.json(policy) : response(method, path);
  };
  const receipts: Receipt[] = [];
  const queue = new UploadQueue("alice", "operations", "tenant", (receipt) => receipts.push(receipt));
  return { memory, file, key, uploadId, receipt, status, queue, calls, receipts, respond: (value: typeof response) => { response = value; } };
}

async function until(check: () => boolean) {
  const deadline = Date.now() + 3000;
  while (!check()) { assert.ok(Date.now() < deadline, "queue did not settle"); await new Promise((resolve) => setTimeout(resolve, 5)); }
}

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
    if (method === "POST") { sealed = true; return Response.json({ ...f.status, state: "finalizing", accepted_bytes: 16, accepted_part_count: 2 }, { status: 202 }); }
    return Response.json(sealed ? { ...f.status, state: "completed", accepted_bytes: 16, accepted_part_count: 2, receipt: f.receipt } : f.status);
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
        this.responseText = JSON.stringify({ part_number: 2, byte_len: blob.size, sha256: sha });
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
