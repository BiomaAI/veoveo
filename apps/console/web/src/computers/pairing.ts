import { z } from "zod";
import { boundedJson, consoleJson } from "../consoleHttp.ts";
import { parseComputer } from "../generatedContracts.ts";
import { revokeAccess } from "./api.ts";
import type { CliPairingResult } from "../generated/computers.ts";

export interface PairingLocation { computerId: string; callbackPort: number; code: string }
export function pairingLocation(url: URL): PairingLocation | undefined {
  const id = url.pathname.match(/^\/console\/computers\/([0-9a-f-]{36})\/auth\/connect$/)?.[1];
  if (!id || !z.uuid().safeParse(id).success || /^0+-0+-0+-0+-0+$/.test(id) || url.hash) return;
  if ([...url.searchParams.keys()].length !== 2 || url.searchParams.getAll("callback_port").length !== 1 || url.searchParams.getAll("code").length !== 1) return;
  const port = url.searchParams.get("callback_port")!;
  if (!/^[1-9][0-9]{3,4}$/.test(port)) return;
  try {
    const input = parseComputer("cli_pairing_input", { name: "CLI", callbackPort: Number(port), code: url.searchParams.get("code") });
    return { computerId: id, callbackPort: input.callbackPort, code: input.code };
  } catch { return; }
}

export class PairingFailure extends Error {}
export async function pairCli(location: PairingLocation, name: string): Promise<void> {
  const input = parseComputer("cli_pairing_input", { name: name.trim(), code: location.code, callbackPort: location.callbackPort });
  if (new TextEncoder().encode(input.name).length > 64) throw new PairingFailure("Use a shorter name for this CLI.");
  // Resolve browser loopback permission before issuing a credential. The stock
  // callback's OPTIONS handler has no grant or pairing side effects.
  try {
    const local = await fetch(`http://127.0.0.1:${location.callbackPort}/callback`, {
      method: "OPTIONS", mode: "cors", credentials: "omit", cache: "no-store", redirect: "error",
      signal: AbortSignal.timeout(60_000),
    });
    if (local.status !== 204) throw new Error("local CLI unavailable");
  } catch {
    throw new PairingFailure("The local CLI could not be reached. Allow apps on this device in your browser’s site permissions, then start CLI login again. No access was issued.");
  }
  const base = `computers/${location.computerId}/cli-pairings`;
  let grant: CliPairingResult | undefined;
  let confirming = false;
  try {
    const challenge = parseComputer("cli_pairing_challenge", await consoleJson(base, input));
    if (challenge.computerId !== location.computerId || Date.parse(challenge.expiresAt) <= Date.now()) throw new PairingFailure("The pairing request expired. Run CLI login again.");
    confirming = true;
    grant = parseComputer("cli_pairing_result", await consoleJson(`${base}/${challenge.pairingId}/confirm`, {}));
    if (grant.computerId !== location.computerId || grant.pairingId !== challenge.pairingId || grant.callbackPort !== location.callbackPort || Date.parse(grant.expiresAt) <= Date.now()) throw new PairingFailure("The pairing response could not be verified.");
    const callback = await fetch(`http://127.0.0.1:${grant.callbackPort}/callback`, {
      method: "POST", mode: "cors", credentials: "omit", cache: "no-store", redirect: "error",
      headers: { "Content-Type": "application/json" }, signal: AbortSignal.timeout(10_000),
      body: JSON.stringify({ token: grant.token, code: location.code }),
    });
    if (!callback.ok || !z.strictObject({ ok: z.literal(true) }).safeParse(await boundedJson(callback, 1024)).success) throw new PairingFailure("The local CLI did not confirm delivery.");
  } catch {
    if (grant?.computerId === location.computerId) {
      try {
        await revokeAccess(location.computerId, grant.grantId);
        throw new PairingFailure("Pairing did not finish. Its access was revoked. Run CLI login again and keep the terminal open.");
      } catch (error) {
        if (error instanceof PairingFailure) throw error;
        throw new PairingFailure("Delivery could not be confirmed. Review this Computer’s access and revoke the named CLI before trying again.");
      }
    }
    throw new PairingFailure(confirming
      ? "The confirmation response was lost or rejected. Review access for this Computer before pairing again."
      : "Pairing could not start. Check current Computer access and run CLI login again.");
  } finally {
    if (grant) grant.token = "";
  }
}
