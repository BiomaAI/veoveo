import { parseComputer } from "../generatedContracts.ts";
import type { MaintenanceView, UpdateTemplateInput } from "../generated/computers.ts";

export interface SavedUpdate {
  input: UpdateTemplateInput;
  receipt?: MaintenanceView;
}
type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;
export function readSavedUpdate(storage: Storage, key: string, computer: string): SavedUpdate | undefined {
  const text = storage.getItem(key);
  if (text === null) return undefined;
  const value: unknown = JSON.parse(text);
  if (!value || typeof value !== "object" || Array.isArray(value)
    || Object.keys(value).some(key => key !== "input" && key !== "receipt") || !("input" in value))
    throw new Error("Invalid saved update");
  const input = parseComputer("update_template_input", value.input);
  const receipt = "receipt" in value ? parseComputer("maintenance_view", value.receipt) : undefined;
  if (input.computerId !== computer || (receipt && (receipt.computerId !== computer
    || (input.templateId && receipt.targetTemplateId !== input.templateId))))
    throw new Error("Saved update identity mismatch");
  return { input, receipt };
}
function same(a: UpdateTemplateInput, b: UpdateTemplateInput): boolean {
  return a.requestId === b.requestId && a.computerId === b.computerId && a.templateId === b.templateId;
}
export function rememberUpdate(storage: Storage, key: string, input: UpdateTemplateInput): void {
  parseComputer("update_template_input", input);
  const prior = readSavedUpdate(storage, key, input.computerId);
  if (prior && !same(prior.input, input)) throw new Error("Resolve the saved update first");
  if (!prior) storage.setItem(key, JSON.stringify({ input }));
}
export async function sendSavedUpdate(
  storage: Storage, key: string, input: UpdateTemplateInput,
  send: (input: UpdateTemplateInput) => Promise<MaintenanceView>,
): Promise<{ receipt: MaintenanceView; saved: boolean }> {
  rememberUpdate(storage, key, input);
  const receipt = parseComputer("maintenance_view", await send(input));
  if (receipt.computerId !== input.computerId || (input.templateId && receipt.targetTemplateId !== input.templateId))
    throw new Error("Update receipt identity mismatch");
  // Another tab/scope cannot replace a saved intent through a late response.
  try {
    const prior = readSavedUpdate(storage, key, input.computerId);
    if (prior && same(prior.input, input)) {
      storage.setItem(key, JSON.stringify({ input, receipt }));
      return { receipt, saved: true };
    }
  } catch { /* A local persistence failure cannot erase a confirmed server reply. */ }
  return { receipt, saved: false };
}
export function updateFinished(view?: MaintenanceView): boolean {
  return view?.phase === "succeeded" || view?.phase === "cancelled";
}
