import { z } from "zod";
import { compileGeneratedSchema } from "../jsonSchema.ts";
import schema from "../generated/artifact-transfer.schema.json" with { type: "json" };
import type { ArtifactUploadNotification, CreateArtifactUpload, ArtifactUploadReceipt, ArtifactUploadSession, UploadPartReceipt, EffectiveArtifactUploadPolicy } from "../generated/artifact-transfer";
import type { ArtifactUploadReceipt as Receipt, EffectiveArtifactUploadPolicy as Policy } from "../generated/artifact-transfer";
import { formatBytes } from "../format.ts";

const bytes = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const id = z.string().regex(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);

function ownerSchema<Model>(name: keyof typeof schema.$defs): z.ZodType<Model> {
  const validator = compileGeneratedSchema({ $schema: schema.$schema, $defs: schema.$defs, $ref: `#/$defs/${name}` });
  // Upload counters travel through JS numbers; unsafe integers cannot drive slices or progress.
  const safeCounters = (value: unknown): boolean => typeof value === "number"
    ? Number.isSafeInteger(value) && value >= 0
    : value === null || typeof value !== "object" || Object.values(value).every(safeCounters);
  return z.custom<Model>((value) => validator.safeParse(value).success && safeCounters(value));
}

// A selected browser File always supplies a known, safely representable length.
export type Descriptor = Omit<CreateArtifactUpload, "byte_len"> & { byte_len: number };
export type { ArtifactUploadReceipt as Receipt, ArtifactUploadSession as Session, UploadPartReceipt as Part, EffectiveArtifactUploadPolicy as Policy } from "../generated/artifact-transfer";
export const descriptorSchema = ownerSchema<CreateArtifactUpload>("CreateArtifactUpload").refine(
  (value): value is Descriptor => Number.isSafeInteger(value.byte_len) && (value.byte_len ?? -1) >= 0 && value.filename.length > 0 && value.filename.length <= 255,
).transform((value) => value as Descriptor);
export const receiptSchema = ownerSchema<ArtifactUploadReceipt>("ArtifactUploadReceipt");
export const partSchema = ownerSchema<UploadPartReceipt>("UploadPartReceipt").refine((part) => part.part_number <= 10000);
export const sessionSchema = ownerSchema<ArtifactUploadSession>("ArtifactUploadSession").refine((session) =>
  session.layout.max_parts <= 10000 && session.parts.length <= 256
  && session.parts.every((part) => part.part_number <= 10000)
  && (session.next_part_cursor === undefined || session.next_part_cursor === null || session.next_part_cursor <= 10000));
export const policySchema = ownerSchema<EffectiveArtifactUploadPolicy>("EffectiveArtifactUploadPolicy").refine((effective) =>
  !effective.policy || (effective.policy.max_parts <= 10000 && effective.policy.part_timeout_seconds <= 3600));
// The Console stream excludes open uploads; a notification wakes an authoritative status read.
export const uploadNotificationSchema = ownerSchema<ArtifactUploadNotification>("ArtifactUploadNotification");
export type Phase = "Selected" | "Queued" | "Preparing" | "Uploading" | "Paused" | "Waiting for connection" | "Sign in to continue" | "Select file" | "Checking file" | "Finishing upload" | "Ready" | "Needs attention" | "Cancelling" | "Cancelled";

export interface Entry {
  key: string;
  descriptor: Descriptor;
  lastModified: number;
  phase: Phase;
  uploadId?: string;
  receipt?: Receipt;
  accepted: number;
  sent: number;
  speed?: number;
  eta?: number;
  message?: string;
  file?: File;
  checked?: boolean;
  cancelRequested?: boolean;
  admissionStarted?: boolean;
  restartRequired?: boolean;
}

export const savedSchema = z.array(z.object({
  key: id, descriptor: descriptorSchema, lastModified: bytes,
  uploadId: id.optional(), receipt: receiptSchema.optional(), accepted: bytes,
  cancelRequested: z.boolean().optional(), cancelled: z.boolean().optional(), admissionStarted: z.boolean().optional(),
  restartRequired: z.boolean().optional(),
})).max(200);

export function requestId(): string {
  const data = crypto.getRandomValues(new Uint8Array(16));
  let time = Date.now();
  for (let index = 5; index >= 0; index--) { data[index] = time % 256; time = Math.floor(time / 256); }
  data[6] = (data[6] & 15) | 0x70;
  data[8] = (data[8] & 63) | 0x80;
  const hex = Array.from(data, (value) => value.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

const extensions: Record<string, string> = {
  csv: "text/csv", parquet: "application/vnd.apache.parquet", json: "application/json",
  jsonl: "application/x-ndjson", ndjson: "application/x-ndjson", txt: "text/plain",
  md: "text/markdown", markdown: "text/markdown",
  png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", webp: "image/webp", pdf: "application/pdf",
  mp4: "video/mp4", zip: "application/zip", bin: "application/octet-stream", rrd: "application/octet-stream",
};
export function describe(file: File): Descriptor {
  return { filename: file.name, byte_len: file.size, mime_type: extensions[file.name.split(".").pop()?.toLowerCase() ?? ""] ?? (file.type.toLowerCase() || "application/octet-stream") };
}
export function invalidSelection(descriptor: Descriptor, policy?: Policy): string | undefined {
  if (!policy?.allowed || !policy.policy) return policy?.explanation ?? "Upload policy is not available yet.";
  if (!Number.isSafeInteger(descriptor.byte_len) || descriptor.byte_len < 0 || descriptor.byte_len > policy.policy.max_object_bytes) return "This file exceeds the upload size limit.";
  if (!descriptor.filename || [".", ".."].includes(descriptor.filename) || new TextEncoder().encode(descriptor.filename).length > 255 || descriptor.filename.trim() !== descriptor.filename || Array.from(descriptor.filename).some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127 || char === "/" || char === "\\")) return "Rename this file to remove unsupported characters or shorten its name.";
  if (!policy.policy.allowed_mime_types.includes(descriptor.mime_type)) return "This file type is not allowed here.";
  if (policy.available_bytes != null && descriptor.byte_len > policy.available_bytes) return `This file requires ${formatBytes(descriptor.byte_len)}. Only ${formatBytes(policy.available_bytes)} of storage is currently available.`;
}

export function duplicate(left: Entry, file: File): boolean {
  return left.descriptor.filename === file.name && left.descriptor.byte_len === file.size && left.lastModified === file.lastModified && left.phase !== "Cancelled";
}
