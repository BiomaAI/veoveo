import { consoleJson, ConsoleHttpError } from "../consoleHttp.ts";
import { parseComputer } from "../generatedContracts.ts";
import type { Action, ApiError, IssueAutomationGrantInput, OperationReceipt, UpdateTemplateInput, ResumeUpdateInput } from "../generated/computers.ts";

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
  grantId?: string,
): Promise<OperationReceipt> {
  if (action !== "create" && !computerId) throw new Error("Select a Computer first.");
  if (action === "create" && grantId) throw new Error("A grant cannot create a Computer.");
  const path =
    action === "create" ? "computers" : `computers/${encodeURIComponent(computerId!)}/${action}`;
  const body =
    action === "create" ? { requestId, ...(computerId ? { computerId } : {}) } : { requestId, ...(grantId ? { grantId } : {}) };
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
export async function readMaintenance(computerId: string, signal?: AbortSignal) {
  const state = parseComputer("maintenance_state", await consoleJson(
    `computers/${encodeURIComponent(computerId)}/maintenance`, undefined, signal,
  ));
  if (state.computerId !== computerId || (state.active && state.active.computerId !== computerId))
    throw new Error("The environment update state could not be verified.");
  return state;
}
export async function readMaintenanceOperation(computerId: string, taskId: string, signal?: AbortSignal) {
  const value = parseComputer("maintenance_view", await consoleJson(
    `computers/${encodeURIComponent(computerId)}/maintenance/${encodeURIComponent(taskId)}`, undefined, signal,
  ));
  if (value.computerId !== computerId || value.taskId !== taskId)
    throw new Error("The environment update receipt could not be verified.");
  return value;
}
export async function resumeUpdate(input: ResumeUpdateInput) {
  parseComputer("resume_update_input", input);
  const value = parseComputer("maintenance_view", await consoleJson(
    `computers/${encodeURIComponent(input.computerId)}/maintenance/${encodeURIComponent(input.taskId)}/resume`, input,
  ));
  if (value.computerId !== input.computerId || value.taskId !== input.taskId)
    throw new Error("The recovery response could not be verified.");
  return value;
}
export async function updateTemplate(input: UpdateTemplateInput) {
  parseComputer("update_template_input", input);
  const value = parseComputer("maintenance_view", await consoleJson(
    `computers/${encodeURIComponent(input.computerId)}/update-template`, input,
  ));
  if (value.computerId !== input.computerId || (input.templateId && input.templateId !== value.targetTemplateId))
    throw new Error("The environment update receipt could not be verified.");
  return value;
}
export async function readAccessGrants(computerId: string, signal?: AbortSignal) {
  const grants = parseComputer("access_grants", await consoleJson(
    `computers/${encodeURIComponent(computerId)}/access`, undefined, signal,
  ));
  if (grants.computerId !== computerId)
    throw new Error("The access inventory could not be verified.");
  return grants;
}
export async function revokeAccess(computerId: string, grantId: string, signal?: AbortSignal) {
  const receipt = parseComputer("access_revocation", await consoleJson(
    `computers/${encodeURIComponent(computerId)}/access/${encodeURIComponent(grantId)}/revoke`,
    {}, signal,
  ));
  if (receipt.computerId !== computerId || receipt.grantId !== grantId || !receipt.revoked)
    throw new Error("The access revocation could not be verified.");
  return receipt;
}
export async function readAutomation(computerId: string, signal?: AbortSignal) {
  const value = parseComputer("automation_grants", await consoleJson(
    `computers/${encodeURIComponent(computerId)}/automation`, undefined, signal,
  ));
  if (value.computerId !== computerId || value.grants.some(g => g.computerId !== computerId))
    throw new Error("The automation inventory could not be verified.");
  return value;
}
function automationResult(value: unknown, computerId: string, grantId?: string) {
  const result = parseComputer("automation_grant_result", value);
  if (result.grant.computerId !== computerId || (grantId && result.grant.grantId !== grantId)
    || result.result_uri !== `computer://computers/${computerId}/automation/${result.grant.grantId}`)
    throw new Error("The automation grant could not be verified.");
  return result;
}
export async function grantAutomation(input: IssueAutomationGrantInput) {
  parseComputer("issue_automation_grant", input);
  return automationResult(await consoleJson(`computers/${encodeURIComponent(input.computerId)}/automation`, input), input.computerId);
}
export async function revokeAutomation(computerId: string, grantId: string) {
  const result = automationResult(await consoleJson(
    `computers/${encodeURIComponent(computerId)}/automation/${encodeURIComponent(grantId)}/revoke`, {},
  ), computerId, grantId);
  if (!result.grant.revokedAt) throw new Error("The revocation could not be verified.");
  return result;
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
        invalid_input: "The request was rejected. Check its values and application registration before submitting a new request.",
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
