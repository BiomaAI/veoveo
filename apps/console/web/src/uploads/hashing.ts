export class Hashing {
  private worker = new Worker(new URL("./hash.worker.ts", import.meta.url), { type: "module" });
  private next = 0;
  private jobs = new Map<number, { resolve: (sha: string) => void; reject: (error: Error) => void }>();
  constructor(signal: AbortSignal) {
    const stop = () => this.dispose();
    signal.addEventListener("abort", stop, { once: true });
    this.worker.onmessage = (event: MessageEvent<{ id: number; sha?: string; error?: string }>) => {
      const job = this.jobs.get(event.data.id);
      if (!job) return;
      this.jobs.delete(event.data.id);
      if (event.data.sha) job.resolve(event.data.sha); else job.reject(new Error(event.data.error ?? "File hashing failed."));
    };
    this.worker.onerror = () => this.dispose(new Error("File hashing failed. Select the file again."));
    this.cleanup = () => signal.removeEventListener("abort", stop);
  }
  private cleanup: () => void;
  hash(blob: Blob): Promise<string> {
    const id = ++this.next;
    return new Promise((resolve, reject) => { this.jobs.set(id, { resolve, reject }); this.worker.postMessage({ id, blob }); });
  }
  dispose(error: Error = new DOMException("Upload paused", "AbortError")): void {
    this.worker.terminate(); this.cleanup();
    for (const job of this.jobs.values()) job.reject(error);
    this.jobs.clear();
  }
}
