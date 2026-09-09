import { z } from "zod";
import { Hashing } from "./hashing.ts";
import { describe, duplicate, invalidSelection, policySchema, requestId, savedSchema, sessionSchema, type Entry, type Part, type Policy, type Receipt, type Session } from "./model.ts";
import { delay, putPart, request, UploadError } from "./transport.ts";

export interface QueueState { entries: Entry[]; policy?: Policy; policyError?: string; notice?: string; persistenceError?: string }

/** One file's negotiated parallel part window bounds browser memory across the queue. */
export class UploadQueue {
  private state: QueueState = { entries: [] };
  private listeners = new Set<() => void>();
  private lifecycle = new AbortController();
  private active?: { key: string; controller: AbortController };
  private watching = new Map<string, AbortController>();
  private disposed = true;
  private storageKey: string;
  private actor: string;
  private context: string;
  private onReceipt: (receipt: Receipt) => void;

  constructor(actor: string, context: string, tenant: string, onReceipt: (receipt: Receipt) => void) {
    this.actor = actor; this.context = context; this.onReceipt = onReceipt;
    this.storageKey = `veoveo.uploads.v1:${JSON.stringify([location.origin, tenant, actor, context])}`;
    try {
      const raw = localStorage.getItem(this.storageKey);
      if (raw && raw.length <= 512 * 1024) {
        const saved = savedSchema.parse(JSON.parse(raw));
        this.state.entries = saved.map(({ cancelled, ...entry }) => ({ ...entry, receipt: undefined, sent: entry.accepted, phase: cancelled ? "Cancelled" : "Select file" }));
      }
    } catch { this.state.persistenceError = "Saved upload information could not be restored."; }
  }
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  snapshot = () => this.state;
  private entry(key: string): Entry | undefined { return this.state.entries.find((entry) => entry.key === key); }
  private emit(persist = true): void {
    this.state = { ...this.state };
    if (persist) {
      try {
        localStorage.setItem(this.storageKey, JSON.stringify(this.state.entries.map((entry) => ({
          key: entry.key, descriptor: entry.descriptor, lastModified: entry.lastModified, uploadId: entry.uploadId,
          receipt: entry.receipt, accepted: entry.accepted, cancelRequested: entry.cancelRequested, cancelled: entry.phase === "Cancelled", admissionStarted: entry.admissionStarted,
        }))));
      } catch { this.state.persistenceError = "This browser cannot save the queue. Keep this tab open until uploads finish."; }
    }
    this.listeners.forEach((listener) => listener());
  }
  private patch(key: string, patch: Partial<Entry>, persist = true): void {
    this.state.entries = this.state.entries.map((entry) => entry.key === key ? { ...entry, ...patch } : entry);
    this.emit(persist);
  }
  async initialize(): Promise<void> {
    this.disposed = false; this.lifecycle = new AbortController();
    window.addEventListener("online", this.online);
    await this.refreshPolicy();
    if (this.disposed || !this.state.policy?.allowed) return;
    for (const entry of this.state.entries) {
      if (this.disposed) break;
      if (!entry.uploadId || entry.phase === "Cancelled") continue;
      try {
        const status = await request(`/${entry.uploadId}`, "GET", sessionSchema, this.lifecycle.signal);
        if (this.disposed) return;
        this.observe(entry.key, status);
        if (status.receipt) continue;
        if (entry.cancelRequested) void this.cancel(entry.key);
        else if (["finalizing", "verifying"].includes(status.state)) this.watch(entry.key);
      } catch (error) { this.failed(entry.key, error); }
    }
  }
  async refreshPolicy(): Promise<void> {
    try {
      const policy = await request("/policy", "GET", policySchema, this.lifecycle.signal);
      if (policy.actor !== this.actor || policy.work_context !== this.context) throw new Error("Your Work Context changed. Refresh Console before uploading.");
      this.state.policy = policy; this.state.policyError = undefined;
      for (const entry of this.state.entries) if (!entry.uploadId && entry.file) {
        const message = invalidSelection(entry.descriptor, policy);
        this.patch(entry.key, { message, phase: message ? "Needs attention" : "Selected" });
      }
      this.emit(false);
    } catch (error) {
      if (this.lifecycle.signal.aborted) return;
      this.state.policy = undefined;
      this.state.policyError = error instanceof Error ? error.message : "Upload policy could not be loaded.";
      this.emit(false);
    }
  }
  select(files: File[], deliberateCopy = false): void {
    this.state.notice = undefined;
    for (const file of files) {
      if (this.state.entries.length >= 200) { this.state.notice = "Clear finished uploads before selecting more files."; break; }
      if (!deliberateCopy && this.state.entries.some((entry) => duplicate(entry, file))) {
        this.state.notice = `${file.name} is already in the queue. Use Upload another copy to create a new artifact.`; continue;
      }
      const descriptor = describe(file);
      const message = invalidSelection(descriptor, this.state.policy);
      this.state.entries = [...this.state.entries, { key: requestId(), descriptor, lastModified: file.lastModified, file, phase: message ? "Needs attention" : "Selected", accepted: 0, sent: 0, message }];
    }
    this.emit();
  }
  remove(key: string): void {
    if (this.entry(key)?.uploadId || this.entry(key)?.admissionStarted) return;
    this.state.entries = this.state.entries.filter((entry) => entry.key !== key); this.emit();
  }
  clearCompleted(): void { this.state.entries = this.state.entries.filter((entry) => !["Ready", "Cancelled"].includes(entry.phase)); this.emit(); }
  start(): void {
    for (const entry of this.state.entries) if (entry.phase === "Selected" && !invalidSelection(entry.descriptor, this.state.policy)) this.patch(entry.key, { phase: "Queued", message: undefined });
    this.pump();
  }
  resume(key: string): void {
    const entry = this.entry(key);
    if (!entry || entry.receipt || entry.phase === "Cancelled") return;
    if (entry.cancelRequested) { void this.cancel(key); return; }
    if (!entry.file) {
      if (entry.uploadId) {
        void request(`/${entry.uploadId}`, "GET", sessionSchema, this.lifecycle.signal).then((status) => {
          this.observe(key, status);
          if (["finalizing", "verifying"].includes(status.state)) this.watch(key);
          else if (status.state === "open") this.patch(key, { phase: "Select file", message: "Choose the original file to continue." });
        }).catch((error: unknown) => this.failed(key, error));
      } else this.patch(key, { phase: "Select file", message: "Choose the original file to continue." });
      return;
    }
    this.patch(key, { phase: "Queued", message: undefined }); this.pump();
  }
  reselect(key: string, file: File): void {
    const entry = this.entry(key);
    if (!entry) return;
    if (file.size !== entry.descriptor.byte_len) { this.patch(key, { phase: "Select file", message: "This file has a different size. Select the original file; saved progress is unchanged." }); return; }
    this.patch(key, { file, checked: false, phase: "Queued", message: undefined }); this.pump();
  }
  pause(key: string): void {
    const entry = this.entry(key);
    if (!entry || ["Ready", "Cancelled", "Finishing upload", "Cancelling"].includes(entry.phase)) return;
    if (this.active?.key === key) this.active.controller.abort();
    this.patch(key, { phase: "Paused", sent: entry.accepted, speed: undefined, eta: undefined });
  }
  pauseAll(dropFiles = false): void {
    for (const entry of this.state.entries) {
      this.pause(entry.key);
      if (dropFiles) this.patch(entry.key, { file: undefined, checked: false });
    }
  }
  async cancel(key: string): Promise<void> {
    const entry = this.entry(key);
    if (!entry || entry.receipt) return;
    if (!entry.uploadId && !entry.admissionStarted && this.active?.key !== key) { this.remove(key); return; }
    if (this.active?.key === key) this.active.controller.abort();
    this.watching.get(key)?.abort();
    this.patch(key, { cancelRequested: true, phase: "Cancelling", message: undefined });
    try {
      // An admission acknowledgement can be lost. Replay its key before cancelling.
      const status = entry.uploadId ? await request(`/${entry.uploadId}`, "GET", sessionSchema, this.lifecycle.signal)
        : await request("", "POST", sessionSchema, this.lifecycle.signal, entry.descriptor, key);
      this.patch(key, { uploadId: status.upload_id });
      if (status.receipt) { this.observe(key, status); return; }
      try { await request(`/${status.upload_id}`, "DELETE", z.undefined(), this.lifecycle.signal); }
      catch (error) {
        if (!(error instanceof UploadError) || error.status !== 409) throw error;
        const current = await request(`/${status.upload_id}`, "GET", sessionSchema, this.lifecycle.signal);
        if (current.receipt) { this.observe(key, current); return; }
        throw error;
      }
      this.patch(key, { phase: "Cancelled", cancelRequested: false, file: undefined, sent: entry.accepted });
    } catch (error) {
      if (!this.lifecycle.signal.aborted) this.patch(key, { phase: "Cancelling", message: error instanceof Error ? `${error.message} Retry cancellation when connected.` : "Cancellation is waiting for a connection." });
    }
  }
  private online = (): void => {
    for (const entry of this.state.entries) {
      if (entry.phase === "Waiting for connection") this.resume(entry.key);
      if (entry.cancelRequested) void this.cancel(entry.key);
    }
  };
  private pump(): void {
    if (this.disposed || this.active) return;
    const entry = this.state.entries.find((entry) => entry.phase === "Queued");
    if (!entry) return;
    const controller = new AbortController();
    this.active = { key: entry.key, controller };
    void this.transfer(entry.key, controller).catch((error: unknown) => this.failed(entry.key, error)).finally(() => {
      if (this.active?.controller === controller) this.active = undefined;
      this.pump();
    });
  }
  private observe(key: string, status: Session): void {
    const entry = this.entry(key);
    if (!entry) return;
    this.patch(key, { uploadId: status.upload_id, accepted: status.accepted_bytes, sent: status.accepted_bytes });
    if (status.receipt) {
      if (status.receipt.upload_id !== status.upload_id || status.receipt.byte_len !== entry.descriptor.byte_len || status.receipt.filename !== entry.descriptor.filename) throw new Error("The completed upload receipt does not match this file.");
      this.patch(key, { receipt: status.receipt, phase: "Ready", file: undefined, checked: false, message: undefined, cancelRequested: false, speed: undefined, eta: undefined });
      this.onReceipt(status.receipt);
    } else if (["finalizing", "verifying"].includes(status.state)) this.patch(key, { phase: "Finishing upload", speed: undefined, eta: undefined });
    else if (status.state === "cancelled") this.patch(key, { phase: "Cancelled", file: undefined, cancelRequested: false });
    else if (["expired", "failed"].includes(status.state)) this.patch(key, { phase: "Needs attention", message: status.state === "expired" ? "This upload expired. Start a new upload." : "File verification failed. Start a new upload." });
  }
  private failed(key: string, error: unknown): void {
    if (this.disposed || (error instanceof DOMException && error.name === "AbortError")) return;
    const entry = this.entry(key);
    if (!entry || entry.phase === "Cancelling" || entry.phase === "Cancelled") return;
    if (error instanceof UploadError && error.status === 401) {
      this.pauseAll(true);
      this.patch(key, { phase: "Sign in to continue", message: error.message });
    } else if (error instanceof UploadError && (error.status === 0 || !navigator.onLine)) {
      this.patch(key, { phase: "Waiting for connection", message: "Accepted parts are saved. Transfer can resume when connected.", sent: entry.accepted, eta: undefined });
    } else this.patch(key, { phase: "Needs attention", message: error instanceof Error ? error.message : "Upload interrupted. Retry to recover saved progress.", sent: entry.accepted, eta: undefined });
  }
  private async transfer(key: string, controller: AbortController): Promise<void> {
    const signal = controller.signal;
    let entry = this.entry(key);
    if (!entry?.file) { this.patch(key, { phase: "Select file" }); return; }
    const file = entry.file;
    this.patch(key, { phase: "Preparing", message: undefined });
    if (!entry.uploadId) this.patch(key, { admissionStarted: true });
    let status = entry.uploadId ? await request(`/${entry.uploadId}`, "GET", sessionSchema, signal)
      : await request("", "POST", sessionSchema, signal, entry.descriptor, key);
    signal.throwIfAborted(); this.observe(key, status);
    if (status.state !== "open") { if (["finalizing", "verifying"].includes(status.state)) this.watch(key); return; }
    const parts = new Map<number, Part>(status.parts.map((part) => [part.part_number, part]));
    while (status.next_part_cursor) {
      const cursor = status.next_part_cursor;
      status = await request(`/${status.upload_id}?after=${cursor}`, "GET", sessionSchema, signal);
      if (status.next_part_cursor && status.next_part_cursor <= cursor) throw new Error("Upload status cursor did not advance.");
      status.parts.forEach((part) => parts.set(part.part_number, part));
      if (parts.size > status.layout.max_parts) throw new Error("Upload status exceeds its part limit.");
    }
    const hashing = new Hashing(signal);
    try {
      if (!entry.checked && parts.size) {
        this.patch(key, { phase: "Checking file", message: "Checking this file against the saved parts before sending bytes." });
        for (const part of parts.values()) {
          signal.throwIfAborted();
          const offset = (part.part_number - 1) * status.layout.part_bytes;
          if (await hashing.hash(file.slice(offset, offset + part.byte_len)) !== part.sha256) {
            this.patch(key, { file: undefined, checked: false });
            throw new Error("This file does not match the saved parts. Select the original file; saved progress is unchanged.");
          }
        }
      }
      this.patch(key, { checked: true, phase: "Uploading", message: undefined });
      const count = Math.max(1, Math.ceil(file.size / status.layout.part_bytes));
      if (count > status.layout.max_parts || file.size > status.layout.max_total_bytes) throw new Error("This file exceeds the admitted upload layout.");
      const missing = Array.from({ length: count }, (_, index) => index + 1).filter((number) => !parts.has(number));
      let next = 0;
      const inFlight = new Map<number, number>();
      const accepted = () => [...parts.values()].reduce((sum, part) => sum + part.byte_len, 0);
      const began = performance.now();
      const initialAccepted = accepted();
      let samples = 0;
      const progress = () => {
        const sent = accepted() + [...inFlight.values()].reduce((sum, value) => sum + value, 0);
        const seconds = (performance.now() - began) / 1000;
        const speed = seconds > 3 && ++samples >= 3 ? (sent - initialAccepted) / seconds : undefined;
        this.patch(key, { accepted: accepted(), sent, speed, eta: speed && speed > 0 ? Math.ceil((file.size - sent) / speed) : undefined }, false);
      };
      const upload = async () => {
        while (next < missing.length) {
          signal.throwIfAborted();
          const number = missing[next++];
          const offset = (number - 1) * status.layout.part_bytes;
          const blob = file.slice(offset, Math.min(file.size, offset + status.layout.part_bytes));
          const sha = await hashing.hash(blob);
          let receipt: Part | undefined;
          for (let attempt = 0; attempt < 4; attempt++) {
            signal.throwIfAborted();
            try {
              receipt = await putPart(status.upload_id, number, blob, sha, this.state.policy?.policy?.part_timeout_seconds ?? 120, signal, (loaded) => { inFlight.set(number, loaded); progress(); });
              break;
            } catch (error) {
              inFlight.delete(number); progress();
              if (!(error instanceof UploadError) || ![0, 429, 503, 502, 504].includes(error.status) || attempt === 3 || !navigator.onLine) throw error;
              await delay(Math.max(error.retryAfter, 2 ** attempt) * 1000, signal);
            }
          }
          signal.throwIfAborted();
          if (!receipt || receipt.part_number !== number || receipt.byte_len !== blob.size || receipt.sha256 !== sha) throw new Error("The accepted part receipt does not match this file.");
          parts.set(number, receipt); inFlight.delete(number); progress(); this.emit();
        }
      };
      let failure: unknown;
      const budget = this.state.policy?.policy?.max_inflight_bytes;
      const parallel = budget ? Math.min(status.layout.parallel_parts, Math.floor(budget / status.layout.part_bytes), missing.length) : 0;
      if (missing.length && parallel < 1) throw new Error("Current upload memory policy cannot admit this file's transfer layout.");
      await Promise.all(Array.from({ length: parallel }, () => upload().catch((error: unknown) => {
        if (!failure) { failure = error; controller.abort(); }
      })));
      if (failure) throw failure;
      signal.throwIfAborted();
      this.patch(key, { phase: "Finishing upload", eta: undefined, speed: undefined });
      status = await request(`/${status.upload_id}/complete`, "POST", sessionSchema, signal, { byte_len: file.size, part_count: count });
      this.observe(key, status);
      if (!status.receipt) this.watch(key);
    } finally { hashing.dispose(); }
    entry = this.entry(key);
    if (entry?.phase === "Finishing upload") this.patch(key, { file: undefined });
  }
  private watch(key: string): void {
    this.watching.get(key)?.abort();
    const controller = new AbortController(); this.watching.set(key, controller);
    void (async () => {
      while (!this.disposed) {
        const id = this.entry(key)?.uploadId;
        if (!id) return;
        const status = await request(`/${id}`, "GET", sessionSchema, controller.signal);
        controller.signal.throwIfAborted(); this.observe(key, status);
        if (!["finalizing", "verifying"].includes(status.state)) return;
        await delay(2000, controller.signal);
      }
    })().catch((error: unknown) => this.failed(key, error)).finally(() => { if (this.watching.get(key) === controller) this.watching.delete(key); });
  }
  dispose(): void {
    this.disposed = true; this.lifecycle.abort();
    this.active?.controller.abort(); this.watching.forEach((controller) => controller.abort());
    window.removeEventListener("online", this.online); this.pauseAll(true);
  }
}
