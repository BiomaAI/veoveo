import { browserApiRoot } from "./browserApp.ts";
import { authenticationRequired } from "./auth.ts";
import { acceptBrowserCsrfToken, browserCsrfToken } from "./csrf.ts";
import { httpErrorMessage, sessionNotReadyMessage } from "./httpMessages.ts";

export class BrowserHttpError extends Error {
  readonly status: number;
  readonly payload: unknown;
  constructor(status: number, payload: unknown) {
    super(httpErrorMessage(status, { action: "complete this request" }));
    this.status = status;
    this.payload = payload;
  }
}
export async function boundedJson(response: Response, limit: number, expectedByteLength?: number): Promise<unknown> {
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
    if (expectedByteLength !== undefined && size !== expectedByteLength) throw new Error("The response did not match its declared size.");
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
export async function browserJson(
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<unknown> {
  const mutation = body !== undefined;
  const csrf = browserCsrfToken();
  if (mutation && !csrf) throw new Error(sessionNotReadyMessage);
  const headers = new Headers({ Accept: "application/json" });
  if (mutation) {
    headers.set("Content-Type", "application/json");
    headers.set("X-Veoveo-CSRF-Token", csrf!);
  }
  const deadline = AbortSignal.timeout(mutation ? 35_000 : 15_000);
  const response = await fetch(`${browserApiRoot()}/${path}`, {
    method: mutation ? "POST" : "GET",
    credentials: "same-origin",
    cache: "no-store",
    redirect: "error",
    headers,
    body: mutation ? JSON.stringify(body) : undefined,
    signal: signal ? AbortSignal.any([signal, deadline]) : deadline,
  });
  acceptBrowserCsrfToken(response.headers.get("x-veoveo-csrf-token"));
  if (response.status === 401) authenticationRequired();
  const payload = await boundedJson(response, 2 * 1024 * 1024).catch((error: unknown) => {
    if (response.ok) throw error;
    return undefined;
  });
  if (!response.ok) throw new BrowserHttpError(response.status, payload);
  return payload;
}
