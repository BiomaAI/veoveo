import { consoleJson, ConsoleHttpError } from "../consoleHttp.ts";
import { parseComputer } from "../generatedContracts.ts";
import type { Action, ApiError, OperationReceipt } from "../generated/computers.ts";

export async function readComputers(after?: string, signal?: AbortSignal) {
  return parseComputer(
    "snapshot",
    await consoleJson(
      `computers${after ? `?after=${encodeURIComponent(after)}` : ""}`,
      undefined,
      signal,
    ),
  );
}
export async function lifecycle(
  action: Action,
  requestId: string,
  computerId?: string,
  signal?: AbortSignal,
): Promise<OperationReceipt> {
  if (action !== "create" && !computerId) throw new Error("Select a Computer first.");
  const path =
    action === "create" ? "computers" : `computers/${encodeURIComponent(computerId!)}/${action}`;
  const body =
    action === "create" ? { requestId, ...(computerId ? { computerId } : {}) } : { requestId };
  parseComputer(
    action === "create" ? "create_input" : action === "start" ? "start_input" : "stop_input",
    body,
  );
  const receipt = parseComputer("receipt", await consoleJson(path, body, signal));
  if (receipt.action !== action || (computerId && receipt.computerId !== computerId))
    throw new Error("The operation response could not be verified.");
  return receipt;
}
export async function readOperation(computerId: string, operationId: string, signal?: AbortSignal) {
  const receipt = parseComputer("receipt", await consoleJson(
    `computers/${encodeURIComponent(computerId)}/operations/${encodeURIComponent(operationId)}`,
    undefined,
    signal,
  ));
  if (receipt.computerId !== computerId || receipt.taskId !== operationId)
    throw new Error("The operation response could not be verified.");
  return receipt;
}
export async function terminalTicket(id: string, signal?: AbortSignal) {
  const ticket = parseComputer(
    "terminal_ticket",
    await consoleJson(`computers/${encodeURIComponent(id)}/terminal-ticket`, {}, signal),
  );
  if (
    ticket.computerId !== id ||
    ticket.endpoint !== `/console/api/computers/${id}/terminal` ||
    Date.parse(ticket.expiresAt) <= Date.now()
  ) {
    throw new Error("The terminal attachment could not be verified.");
  }
  return ticket;
}
export function computerError(error: unknown): string {
  if (error instanceof ConsoleHttpError) {
    let fault: ApiError | undefined;
    try {
      fault = parseComputer("error", error.payload);
    } catch {
      /* Do not display unvalidated upstream text. */
    }
    if (fault) {
      const messages: Partial<Record<ApiError["code"], string>> = {
        busy: "An operation is already in progress on this Computer.",
        capacity_full:
          "Computer capacity is full. Stop an unused Computer or contact your administrator.",
        storage_headroom: "There is not enough reserved storage for this operation.",
        storage_unavailable: "Retained storage is temporarily unavailable.",
        forbidden: "This action is not permitted with your current access.",
        not_found: "This Computer is not available in the current Work Context.",
        invalid_state: "The Computer state changed. Refresh its status before choosing an action.",
        access_limit: "The attachment limit has been reached.",
        ticket_rejected: "This attachment expired or has already been used. Connect again.",
      };
      return (
        messages[fault.code] ?? "The operation could not be confirmed. Retry with the same request."
      );
    }
  }
  return "The request could not be confirmed. Check the connection and retry with the same request.";
}
