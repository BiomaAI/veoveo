import { consoleJson } from "../consoleHttp.ts";
import { parseComputer } from "../generatedContracts.ts";
import type { TransferFileInput } from "../generated/computers.ts";

function receipt(value: unknown, computerId: string, taskId?: string) {
  const view = parseComputer("file_transfer_view", value);
  if (view.computerId !== computerId || (taskId && view.taskId !== taskId)
    || (view.result && (view.result.computerId !== computerId || view.result.transferId !== view.taskId
      || view.result.result_uri !== `computer://transfers/${view.taskId}` || view.result.direction !== view.direction)))
    throw new Error("The file transfer response could not be verified.");
  return view;
}
export async function transferFile(input: TransferFileInput) {
  parseComputer("transfer_file_input", input);
  const view = receipt(await consoleJson(`computers/${encodeURIComponent(input.computerId)}/files`, input), input.computerId);
  if (view.direction !== input.transfer.kind) throw new Error("The file transfer direction could not be verified.");
  return view;
}
export async function readFileTransfer(computerId: string, taskId: string, signal?: AbortSignal) {
  return receipt(await consoleJson(`computers/${encodeURIComponent(computerId)}/files/${encodeURIComponent(taskId)}`, undefined, signal), computerId, taskId);
}
export async function cancelFileTransfer(computerId: string, taskId: string) {
  return receipt(await consoleJson(`computers/${encodeURIComponent(computerId)}/files/${encodeURIComponent(taskId)}/cancel`, {}), computerId, taskId);
}
