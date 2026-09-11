import { z } from "zod";
import { uuidV7 } from "../agentControl.ts";
import { parseComputer } from "../generatedContracts.ts";
import type { Action, ComputerSnapshot, OperationReceipt } from "../generated/computers.ts";
import { computerError, lifecycle, readComputers, readOperation } from "./api.ts";
import { watchComputers, type LiveState } from "./events.ts";

const savedIntent = z
  .object({
    requestId: z.uuid(),
    action: z.enum(["create", "start", "stop"]),
    computerId: z.uuid().optional(),
    grantId: z.uuid().optional(),
    receipt: z.unknown().optional(),
  })
  .strict();
export interface Intent {
  requestId: string;
  action: Action;
  computerId?: string;
  grantId?: string;
  receipt?: OperationReceipt;
  sending?: boolean;
  error?: string;
}
export interface ComputersState {
  snapshot?: ComputerSnapshot;
  loading: boolean;
  stale: boolean;
  live: LiveState;
  error?: string;
  intents: Intent[];
  persistenceError?: string;
}
type StoragePort = Pick<Storage, "getItem" | "setItem">;
export interface ComputerPorts {
  read: typeof readComputers;
  operation: typeof readOperation;
  command: typeof lifecycle;
  watch: typeof watchComputers;
  storage: StoragePort;
}
export class ComputersController {
  private state: ComputersState;
  private listeners = new Set<() => void>();
  private stop = new AbortController();
  private unwatch?: () => void;
  private fetching = false;
  private readingReceipts = new Set<string>();
  private dirty = false;
  private pages = 1;
  private started = false;
  private watchSequence = 0;
  private readonly key: string;
  private readonly ports: ComputerPorts;
  constructor(scope: string, ports: ComputerPorts) {
    this.key = `veoveo.computers.operations:${scope}`;
    this.ports = ports;
    let intents: Intent[] = [];
    let persistenceError: string | undefined;
    try {
      const raw = ports.storage.getItem(this.key);
      if (raw && raw.length > 64 * 1024) throw new Error("Oversize saved operations");
      const saved = z
        .array(savedIntent)
        .max(32)
        .parse(JSON.parse(raw ?? "[]"));
      intents = saved.map((intent) => ({
        ...intent,
        receipt:
          intent.receipt === undefined ? undefined : parseComputer("receipt", intent.receipt),
      }));
      for (const intent of intents) {
        if (
          (intent.action !== "create" && !intent.computerId) ||
          (intent.action === "create" && !!intent.grantId) ||
          (intent.receipt &&
            (intent.receipt.action !== intent.action ||
              (intent.computerId && intent.receipt.computerId !== intent.computerId)))
        )
          throw new Error("Invalid saved operation");
      }
    } catch {
      persistenceError =
        "Saved operation state could not be read. Review current Computers before clearing it.";
    }
    this.state = { loading: true, stale: true, live: "connecting", intents, persistenceError };
  }
  snapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private update(patch: Partial<ComputersState>) {
    if (this.stop.signal.aborted) return;
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  start() {
    if (this.started) return;
    this.started = true;
    if (this.stop.signal.aborted) this.stop = new AbortController();
    this.state = {
      ...this.state,
      intents: this.state.intents.map((intent) => ({ ...intent, sending: false })),
    };
    this.reconnect();
  }
  reconnect = () => {
    if (this.stop.signal.aborted) return;
    const sequence = ++this.watchSequence;
    const epoch = this.stop;
    const current = () =>
      sequence === this.watchSequence && epoch === this.stop && !epoch.signal.aborted;
    this.unwatch?.();
    this.unwatch = this.ports.watch(
      () => {
        if (current()) this.refresh();
      },
      (live) => {
        if (current()) this.update({ live, ...(live !== "live" ? { stale: true } : {}) });
      },
    );
    this.refresh();
  };
  dispose() {
    this.stop.abort();
    this.unwatch?.();
    this.listeners.clear();
    this.started = false;
    this.fetching = false;
    this.readingReceipts.clear();
    this.dirty = false;
  }
  refresh = () => {
    if (this.stop.signal.aborted) return;
    this.dirty = true;
    if (!this.fetching) void this.read();
  };
  loadMore = () => {
    if (this.pages < 10) {
      this.pages++;
      this.refresh();
    }
  };
  private async read() {
    const epoch = this.stop;
    this.fetching = true;
    try {
      while (this.dirty && !epoch.signal.aborted && epoch === this.stop) {
        this.dirty = false;
        try {
          const snapshot = await this.ports.read(undefined, epoch.signal);
          if (epoch.signal.aborted || epoch !== this.stop) return;
          const seen = new Set(snapshot.computers.map((computer) => computer.computerId));
          for (let page = 1; page < this.pages && snapshot.nextCursor; page++) {
            const next = await this.ports.read(snapshot.nextCursor, epoch.signal);
            if (epoch.signal.aborted || epoch !== this.stop) return;
            for (const computer of next.computers) {
              if (!seen.has(computer.computerId)) {
                snapshot.computers.push(computer);
                seen.add(computer.computerId);
              }
            }
            if (snapshot.nextCursor === next.nextCursor)
              throw new Error("Pagination did not advance");
            snapshot.nextCursor = next.nextCursor;
          }
          this.update({
            snapshot,
            loading: false,
            stale: this.state.live !== "live",
            error: undefined,
          });
          await this.refreshReceipts(epoch);
        } catch (error) {
          if (!epoch.signal.aborted && epoch === this.stop)
            this.update({ error: computerError(error), loading: false, stale: true });
        }
      }
    } finally {
      if (epoch === this.stop) this.fetching = false;
    }
  }
  private persist(intents: Intent[]) {
    this.ports.storage.setItem(
      this.key,
      JSON.stringify(
        intents.map(({ requestId, action, computerId, grantId, receipt }) => ({
          requestId,
          action,
          computerId,
          grantId,
          receipt,
        })),
      ),
    );
    this.update({ intents, persistenceError: undefined });
  }
  private async refreshReceipts(epoch: AbortController) {
    const pending = this.state.intents.filter((intent) => intent.receipt && !intent.sending &&
      ["queued", "running", "recovery_required"].includes(intent.receipt.status));
    // Bound read fanout independently of the saved receipt limit. These reads
    // never redispatch a lifecycle command or query the provider.
    for (let offset = 0; offset < pending.length && !epoch.signal.aborted; offset += 4) {
      await Promise.all(pending.slice(offset, offset + 4).map((intent) => this.readReceipt(intent.requestId, epoch)));
    }
  }
  private async readReceipt(requestId: string, epoch = this.stop) {
    const intent = this.state.intents.find((intent) => intent.requestId === requestId);
    const previous = intent?.receipt;
    if (!previous || epoch.signal.aborted || epoch !== this.stop || this.readingReceipts.has(requestId)) return;
    this.readingReceipts.add(requestId);
    try {
      const receipt = await this.ports.operation(previous.computerId, previous.taskId, epoch.signal);
      if (receipt.action !== intent.action) throw new Error("Operation action changed");
      if (epoch.signal.aborted || epoch !== this.stop) return;
      const intents = this.state.intents.map((current) => current.requestId === requestId
        ? { ...current, receipt, error: undefined } : current);
      if (!intents.some((current) => current.requestId === requestId)) return;
      try { this.persist(intents); }
      catch {
        this.update({ intents, persistenceError: "Current operation status could not be saved in this browser." });
      }
    } catch (error) {
      if (!epoch.signal.aborted && epoch === this.stop)
        this.update({intents: this.state.intents.map((current) => current.requestId === requestId
          ? {...current, error: computerError(error)} : current)});
    } finally {
      if (epoch === this.stop) this.readingReceipts.delete(requestId);
    }
  }
  clearSavedAfterReview = () => {
    try {
      this.persist([]);
    } catch {
      this.update({ persistenceError: "The browser could not clear saved recovery requests." });
    }
  };
  command = async (action: Action, computerId?: string) => {
    if (this.state.persistenceError || this.stop.signal.aborted) return;
    const uncertain = this.state.intents.find(
      (intent) => intent.action === action && intent.computerId === computerId && !intent.receipt,
    );
    if (uncertain) return this.retry(uncertain.requestId);
    const computer = this.state.snapshot?.computers.find(c => c.computerId === computerId);
    const granted = action !== "create" && computer?.accessMode === "granted";
    const grantId = granted ? computer.grantedAccess.find(g => g.permissions.includes(action as "start" | "stop")
      && Date.parse(g.expiresAt) > Date.now())?.grantId : undefined;
    if (granted && !grantId) {
      this.update({ error: "Refresh this Computer's granted access before starting another operation." });
      return;
    }
    const intent: Intent = { action, computerId, grantId, requestId: uuidV7() };
    const intents = [...this.state.intents, intent];
    if (intents.length > 32) {
      this.update({ error: "Dismiss saved operation receipts before starting another operation." });
      return;
    }
    try {
      this.persist(intents);
    } catch {
      this.update({
        persistenceError:
          "The browser could not save a recovery request. Free browser storage and retry.",
      });
      return;
    }
    await this.retry(intent.requestId);
  };
  retry = async (requestId: string) => {
    const epoch = this.stop;
    const intent = this.state.intents.find((intent) => intent.requestId === requestId);
    if (!intent || intent.sending || this.stop.signal.aborted) return;
    if (intent.receipt) return this.readReceipt(requestId, epoch);
    this.update({
      intents: this.state.intents.map((candidate) =>
        candidate === intent ? { ...candidate, sending: true, error: undefined } : candidate,
      ),
    });
    try {
      const receipt = await this.ports.command(
        intent.action,
        intent.requestId,
        intent.computerId,
        epoch.signal,
        intent.grantId,
      );
      if (epoch.signal.aborted || epoch !== this.stop) return;
      const intents = this.state.intents.map((candidate) =>
        candidate.requestId === requestId
          ? { ...candidate, receipt, sending: false, error: undefined }
          : candidate,
      );
      try {
        this.persist(intents);
      } catch {
        this.update({
          intents,
          persistenceError:
            "The request was accepted, but its receipt could not be saved. The original request is retained for recovery.",
        });
      }
    } catch (error) {
      if (epoch.signal.aborted || epoch !== this.stop) return;
      this.update({
        intents: this.state.intents.map((candidate) =>
          candidate.requestId === requestId
            ? { ...candidate, sending: false, error: computerError(error) }
            : candidate,
        ),
      });
    }
    this.refresh();
  };
  dismiss = (requestId: string) => {
    const intent = this.state.intents.find((candidate) => candidate.requestId === requestId);
    if (!intent?.receipt || intent.sending) return;
    try {
      this.persist(this.state.intents.filter((candidate) => candidate.requestId !== requestId));
    } catch {
      this.update({ error: "The saved receipt could not be removed." });
    }
  };
}
