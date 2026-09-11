import { parseComputer } from "../generatedContracts.ts";
import type { MaintenanceView, ResumeUpdateInput } from "../generated/computers.ts";
type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;

export function readSavedResume(storage: Storage, key: string, computer: string, task: string): ResumeUpdateInput | undefined {
  const text = storage.getItem(key);
  if (text === null) return undefined;
  const input = parseComputer("resume_update_input", JSON.parse(text));
  if (input.computerId !== computer || input.taskId !== task)
    throw new Error("Saved recovery identity mismatch");
  return input;
}
function same(a: ResumeUpdateInput, b: ResumeUpdateInput): boolean {
  return a.computerId === b.computerId && a.taskId === b.taskId && a.requestId === b.requestId
    && a.expectedUpdatedAt === b.expectedUpdatedAt && a.acknowledgedCancellationAt === b.acknowledgedCancellationAt;
}
export function rememberResume(storage: Storage, key: string, input: ResumeUpdateInput): void {
  parseComputer("resume_update_input", input);
  const prior = readSavedResume(storage, key, input.computerId, input.taskId);
  if (prior && !same(prior, input)) throw new Error("Resolve the saved recovery request first");
  if (!prior) storage.setItem(key, JSON.stringify(input));
}
export async function sendSavedResume(storage: Storage, key: string, input: ResumeUpdateInput,
  send: (input: ResumeUpdateInput) => Promise<MaintenanceView>): Promise<{ receipt: MaintenanceView; cleared: boolean }> {
  rememberResume(storage, key, input);
  const receipt = parseComputer("maintenance_view", await send(input));
  if (receipt.computerId !== input.computerId || receipt.taskId !== input.taskId)
    throw new Error("Recovery response identity mismatch");
  try {
    const prior = readSavedResume(storage, key, input.computerId, input.taskId);
    if (prior && same(prior, input)) { storage.removeItem(key); return { receipt, cleared: true }; }
  } catch { /* Local storage failure cannot erase a known server response. */ }
  return { receipt, cleared: false };
}
