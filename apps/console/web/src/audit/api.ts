import { authenticationRequired } from "../auth.ts";
import { consoleMutation } from "../api.ts";
import { acceptBrowserCsrfToken } from "../csrf.ts";
import { parseAudit } from "../generatedContracts.ts";
import type { AuditDailyQuery, AuditPartition, AuditQuery } from "../generated/audit.ts";

export class AuditViewExpired extends Error {
  constructor() { super("This audit view has expired. Open a new view to continue."); }
}
export class AuditAccessDenied extends Error {
  constructor() { super("Your account does not have access to this audit partition."); }
}
async function read(path: string, signal: AbortSignal): Promise<unknown> {
  const response = await fetch(`/console/api/audit/${path}`, { signal, credentials: "same-origin", headers: { Accept: "application/json" } });
  acceptBrowserCsrfToken(response.headers.get("x-veoveo-csrf-token"));
  if (response.status === 401) authenticationRequired();
  if (response.status === 410) throw new AuditViewExpired();
  if (response.status === 403) throw new AuditAccessDenied();
  if (!response.ok) throw new Error(`Audit could not be loaded (${response.status}).`);
  return response.json();
}
export function queryString(query: AuditQuery | AuditDailyQuery): string {
  return new URLSearchParams({ query: JSON.stringify(query) }).toString();
}
export const loadPartitions = async (signal: AbortSignal) => parseAudit("partitions", await read("partitions", signal));
export const loadRecords = async (query: AuditQuery, view: string, signal: AbortSignal) => parseAudit("page", await read(`records?${queryString(query)}&${new URLSearchParams({ view })}`, signal));
export const loadDailyCounts = async (query: AuditDailyQuery, view: string, signal: AbortSignal) => parseAudit("daily_page", await read(`summary?${queryString(query)}&${new URLSearchParams({ view })}`, signal));
export const streamUrl = (partition: AuditPartition, view: string) => `/console/api/audit/stream?${new URLSearchParams({ partition: JSON.stringify(partition), view })}`;
export const exportUrl = (query: AuditQuery) => `/console/api/audit/export?${queryString({ ...query, cursor: null })}`;

export const openAuditView = async (partition: AuditPartition) => parseAudit("view", await consoleMutation<unknown>("audit/views", { body: JSON.stringify(partition) }));
