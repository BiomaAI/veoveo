import { z } from "zod";
import { browserSession } from "../../../console/web/src/csrf.ts";
import { ApiError } from "../api.ts";
import schema from "../generated/speech.schema.json" with { type: "json" };
import type { DictationSnapshot, TranscriptDocument, TranscriptionOutput } from "../generated/speech.ts";

type Types = { DictationSnapshot: DictationSnapshot; TranscriptDocument: TranscriptDocument; TranscriptionOutput: TranscriptionOutput };
const validators = new Map<keyof Types, z.ZodType>();
export function parseSpeech<K extends keyof Types>(name: K, value: unknown): Types[K] {
  let validator = validators.get(name);
  if (!validator) {
    const definition: object = { $schema: schema.$schema, $defs: schema.$defs, $ref: `#/$defs/${name}` };
    validator = z.fromJSONSchema(definition as Parameters<typeof z.fromJSONSchema>[0]);
    validators.set(name, validator);
  }
  return validator.parse(value) as Types[K];
}

export async function dictation(path: string, method: string, body?: ArrayBuffer | object, signal?: AbortSignal): Promise<DictationSnapshot> {
  if (!browserSession.csrfToken) throw new ApiError(401);
  const binary = body instanceof ArrayBuffer;
  const response = await fetch(`/workspace/api/speech/dictation${path}`, {
    method, credentials: "same-origin", redirect: "error",
    headers: { "Content-Type": binary ? "application/octet-stream" : "application/json", "X-Veoveo-CSRF-Token": browserSession.csrfToken },
    body: body === undefined ? undefined : binary ? body : JSON.stringify(body),
    signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(30_000)]) : AbortSignal.timeout(30_000),
  });
  browserSession.csrfToken = response.headers.get("x-veoveo-csrf-token") ?? browserSession.csrfToken;
  if (!response.ok) {
    if (response.status === 401) { browserSession.csrfToken = undefined; window.dispatchEvent(new Event("workspace-auth-expired")); }
    throw new ApiError(response.status);
  }
  return parseSpeech("DictationSnapshot", await response.json());
}
