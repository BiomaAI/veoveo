import { parseComputer } from "../generatedContracts.ts";
import type { FileTransferView, TransferFileInput } from "../generated/computers.ts";

export interface SavedFileTransfer { input: TransferFileInput; receipt?: FileTransferView }
type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;
export const MAX_FILE_BYTES = 64 * 1024 * 1024;
export function fileFinished(view?: FileTransferView): boolean {
  return !!view && (view.stage === "failed" || view.stage === "cancelled"
    || (view.stage === "completed" && !!view.result));
}
export function retainedPath(value: string): string {
  if (!value || new TextEncoder().encode(value).length > 1024 || /\p{Cc}/u.test(value)
    || value.split("/").some(part => !part || part === "." || part === ".."))
    throw new Error("Use a relative path inside your home, such as project/data.csv. Parent folders must already exist.");
  return value;
}
export function artifactId(value: string): string {
  const id = value.trim().replace(/^artifact:\/\//, "");
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(id))
    throw new Error("Choose an Artifact or enter its Artifact ID or URI.");
  return id;
}
function same(a: TransferFileInput, b: TransferFileInput): boolean {
  return a.requestId === b.requestId && a.computerId === b.computerId && a.grantId === b.grantId
    && a.limits.maximumBytes === b.limits.maximumBytes && a.limits.maximumSeconds === b.limits.maximumSeconds
    && a.limits.onInterruption === b.limits.onInterruption && a.transfer.kind === b.transfer.kind
    && a.transfer.path === b.transfer.path && (a.transfer.kind === "import" && b.transfer.kind === "import"
      ? a.transfer.artifactId === b.transfer.artifactId : a.transfer.kind === "export" && b.transfer.kind === "export"
        && a.transfer.filename === b.transfer.filename && a.transfer.mediaType === b.transfer.mediaType);
}
export function readSavedFile(storage: Storage, key: string, computerId: string): SavedFileTransfer | undefined {
  const text = storage.getItem(key);
  if (text === null) return undefined;
  const value: unknown = JSON.parse(text);
  if (!value || typeof value !== "object" || Array.isArray(value) || !("input" in value)
    || Object.keys(value).some(key => key !== "input" && key !== "receipt")) throw new Error("Invalid saved file transfer");
  const input = parseComputer("transfer_file_input", value.input);
  const receipt = "receipt" in value ? parseComputer("file_transfer_view", value.receipt) : undefined;
  retainedPath(input.transfer.path);
  if (input.computerId !== computerId || (receipt && (receipt.computerId !== computerId
    || receipt.direction !== input.transfer.kind))) throw new Error("Saved file transfer identity mismatch");
  return { input, receipt };
}
export function rememberFile(storage: Storage, key: string, input: TransferFileInput): void {
  parseComputer("transfer_file_input", input);
  const prior = readSavedFile(storage, key, input.computerId);
  if (prior && !same(prior.input, input)) throw new Error("Resolve the saved file transfer first.");
  if (!prior) storage.setItem(key, JSON.stringify({ input }));
}
export async function sendSavedFile(storage: Storage, key: string, input: TransferFileInput,
  send: (input: TransferFileInput) => Promise<FileTransferView>) {
  rememberFile(storage, key, input);
  const original = readSavedFile(storage, key, input.computerId);
  const receipt = parseComputer("file_transfer_view", await send(input));
  if (receipt.computerId !== input.computerId || receipt.direction !== input.transfer.kind
    || (original?.receipt && original.receipt.taskId !== receipt.taskId)) throw new Error("File receipt identity mismatch");
  try {
    const previous = readSavedFile(storage, key, input.computerId);
    if (previous && same(previous.input, input)) {
      storage.setItem(key, JSON.stringify({ input, receipt }));
      return { receipt, saved: true };
    }
  } catch { /* A confirmed receipt remains visible after local persistence fails. */ }
  return { receipt, saved: false };
}
