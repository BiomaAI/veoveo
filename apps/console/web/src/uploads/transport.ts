import { acceptConsoleCsrfToken, consoleCsrfToken } from "../csrf.ts";
import type { z } from "zod";
import { partSchema, type Part } from "./model.ts";

const base = "/console/api/artifact-uploads";
export class UploadError extends Error {
  status: number;
  retryAfter: number;
  constructor(status: number, message: string, retryAfter = 2) { super(message); this.status = status; this.retryAfter = retryAfter; }
}
function failure(status: number, value: unknown, retryAfter?: string | null): UploadError {
  const message = value && typeof value === "object" && "message" in value && typeof value.message === "string" ? value.message : status === 401 ? "Sign in to continue this upload." : `Upload request failed (${status || "connection interrupted"}).`;
  return new UploadError(status, message, Math.min(60, Math.max(1, Number(retryAfter) || 2)));
}
export async function request<T>(path: string, method: string, schema: z.ZodType<T>, signal: AbortSignal, body?: unknown, key?: string): Promise<T> {
  const headers = new Headers({ Accept: "application/json" });
  if (method !== "GET") {
    const csrf = consoleCsrfToken();
    if (!csrf) throw new UploadError(401, "Sign in to continue this upload.");
    headers.set("x-veoveo-csrf-token", csrf);
  }
  if (body !== undefined) headers.set("Content-Type", "application/json");
  if (key) headers.set("Idempotency-Key", key);
  const response = await fetch(base + path, { method, headers, credentials: "same-origin", signal, body: body === undefined ? undefined : JSON.stringify(body) });
  acceptConsoleCsrfToken(response.headers.get("x-veoveo-csrf-token"));
  const value: unknown = response.status === 204 ? undefined : await response.json().catch(() => undefined);
  signal.throwIfAborted();
  if (!response.ok) throw failure(response.status, value, response.headers.get("retry-after"));
  return schema.parse(value);
}
export function putPart(id: string, number: number, blob: Blob, sha: string, timeout: number, signal: AbortSignal, progress: (bytes: number) => void): Promise<Part> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) { reject(signal.reason); return; }
    const csrf = consoleCsrfToken();
    if (!csrf) { reject(new UploadError(401, "Sign in to continue this upload.")); return; }
    const xhr = new XMLHttpRequest();
    const abort = () => xhr.abort();
    const finish = () => signal.removeEventListener("abort", abort);
    xhr.open("PUT", `${base}/${id}/parts/${number}`);
    xhr.timeout = (timeout + 5) * 1000;
    xhr.setRequestHeader("x-veoveo-csrf-token", csrf);
    xhr.setRequestHeader("x-veoveo-part-byte-len", String(blob.size));
    xhr.setRequestHeader("x-veoveo-part-sha256", sha);
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    xhr.upload.onprogress = (event) => progress(Math.min(blob.size, event.loaded));
    xhr.onload = () => {
      finish();
      acceptConsoleCsrfToken(xhr.getResponseHeader("x-veoveo-csrf-token"));
      let value: unknown;
      try { value = JSON.parse(xhr.responseText); } catch { value = undefined; }
      if (xhr.status < 200 || xhr.status >= 300) { reject(failure(xhr.status, value, xhr.getResponseHeader("retry-after"))); return; }
      const parsed = partSchema.safeParse(value);
      if (parsed.success) resolve(parsed.data); else reject(new Error("The upload receipt is invalid."));
    };
    xhr.onerror = xhr.ontimeout = () => { finish(); reject(new UploadError(0, "Connection interrupted. Accepted parts are saved.")); };
    xhr.onabort = () => { finish(); reject(new DOMException("Upload paused", "AbortError")); };
    signal.addEventListener("abort", abort, { once: true });
    xhr.send(blob);
  });
}
export function delay(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) { reject(signal.reason); return; }
    const abort = () => { clearTimeout(timer); reject(signal.reason); };
    const timer = setTimeout(() => { signal.removeEventListener("abort", abort); resolve(); }, ms);
    signal.addEventListener("abort", abort, { once: true });
  });
}
