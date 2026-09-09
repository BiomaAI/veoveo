import { z } from "zod";
import { formatBytes } from "../format.ts";

const bytes = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const positive = bytes.positive();
const id = z.string().regex(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
const sha = z.string().regex(/^[0-9a-f]{64}$/);
export const uploadNotificationSchema = z.object({ op: z.literal("changed"), upload_id: id, state: z.enum(["finalizing", "verifying", "completed", "cancelled", "expired", "failed"]) });
export const descriptorSchema = z.object({ filename: z.string().min(1).max(255), mime_type: z.string(), byte_len: bytes, sha256: sha.optional() });
export const receiptSchema = z.object({
  upload_id: id, artifact_id: id, artifact_uri: z.string().startsWith("artifact://"),
  sha256: sha, byte_len: bytes, mime_type: z.string(), filename: z.string(), created_at: z.string(),
});
export const partSchema = z.object({ part_number: positive.max(10000), byte_len: bytes, sha256: sha });
export const sessionSchema = z.object({
  upload_id: id, state: z.enum(["open", "finalizing", "verifying", "completed", "cancelled", "expired", "failed"]),
  descriptor: descriptorSchema,
  layout: z.object({ part_bytes: positive, max_parts: positive.max(10000), max_total_bytes: positive, parallel_parts: positive }),
  accepted_bytes: bytes, accepted_part_count: bytes, parts: z.array(partSchema).max(256),
  next_part_cursor: positive.max(10000).optional(), created_at: z.string(), expires_at: z.string(),
  receipt: receiptSchema.optional(), failure: z.string().optional(),
});
export const policySchema = z.object({
  allowed: z.boolean(), explanation: z.string(), actor: z.string(), work_context: z.string(),
  destination_name: z.string(), access_description: z.string(), available_bytes: bytes.optional(),
  policy: z.object({
    max_object_bytes: positive, tenant_quota_bytes: positive, max_active_uploads_per_tenant: positive,
    part_bytes: positive, max_part_bytes: positive, max_parts: positive.max(10000), parallel_parts: positive,
    max_inflight_bytes: positive, inactivity_seconds: positive, lifetime_seconds: positive,
    part_timeout_seconds: positive.max(3600), allowed_mime_types: z.array(z.string()),
  }).optional(),
});
export type Descriptor = z.infer<typeof descriptorSchema>;
export type Receipt = z.infer<typeof receiptSchema>;
export type Session = z.infer<typeof sessionSchema>;
export type Part = z.infer<typeof partSchema>;
export type Policy = z.infer<typeof policySchema>;
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
  if (policy.available_bytes !== undefined && descriptor.byte_len > policy.available_bytes) return `This file requires ${formatBytes(descriptor.byte_len)}. Only ${formatBytes(policy.available_bytes)} of storage is currently available.`;
}

export function duplicate(left: Entry, file: File): boolean {
  return left.descriptor.filename === file.name && left.descriptor.byte_len === file.size && left.lastModified === file.lastModified && left.phase !== "Cancelled";
}
