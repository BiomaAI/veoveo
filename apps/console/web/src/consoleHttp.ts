import { authenticationRequired } from "./auth.ts";
import { acceptConsoleCsrfToken, consoleCsrfToken } from "./csrf.ts";

export class ConsoleHttpError extends Error {
  readonly status: number;
  readonly payload: unknown;
  constructor(status: number, payload: unknown) {
    super(
      status === 403
        ? "This action is not permitted with your current access."
        : `Console request failed (${status}).`,
    );
    this.status = status;
    this.payload = payload;
  }
}
export async function boundedJson(response: Response, limit: number): Promise<unknown> {
  if (!response.body) throw new Error("The response is empty.");
  const reader = response.body.getReader();
  let size = 0;
  const chunks: Uint8Array[] = [];
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw new Error("The response exceeded its size limit.");
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)) as unknown;
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
export async function consoleJson(
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<unknown> {
  const mutation = body !== undefined;
  const csrf = consoleCsrfToken();
  if (mutation && !csrf) throw new Error("The Console session is not ready.");
  const headers = new Headers({ Accept: "application/json" });
  if (mutation) {
    headers.set("Content-Type", "application/json");
    headers.set("X-Veoveo-CSRF-Token", csrf!);
  }
  const deadline = AbortSignal.timeout(mutation ? 35_000 : 15_000);
  const response = await fetch(`/console/api/${path}`, {
    method: mutation ? "POST" : "GET",
    credentials: "same-origin",
    cache: "no-store",
    redirect: "error",
    headers,
    body: mutation ? JSON.stringify(body) : undefined,
    signal: signal ? AbortSignal.any([signal, deadline]) : deadline,
  });
  acceptConsoleCsrfToken(response.headers.get("x-veoveo-csrf-token"));
  if (response.status === 401) authenticationRequired();
  const payload = await boundedJson(response, 2 * 1024 * 1024).catch((error: unknown) => {
    if (response.ok) throw error;
    return undefined;
  });
  if (!response.ok) throw new ConsoleHttpError(response.status, payload);
  return payload;
}
