import { parseComputer } from "../generatedContracts.ts";

export type TerminalStatus = "connecting" | "replaying" | "ready" | "disconnected";
export interface TerminalOutput {
  write: (bytes: Uint8Array, done: () => void) => void;
  input: (enabled: boolean) => void;
}
export interface TerminalWire {
  bufferedAmount: number;
  send: (data: string | Uint8Array<ArrayBuffer>) => void;
  close: () => void;
}
export interface TerminalClock {
  wall: () => number;
  monotonic: () => number;
  timer: (callback: () => void, ms: number) => ReturnType<typeof setTimeout>;
  clear: (timer: ReturnType<typeof setTimeout>) => void;
}
const clock: TerminalClock = {
  wall: Date.now,
  monotonic: () => performance.now(),
  timer: (callback, ms) => setTimeout(callback, ms),
  clear: (timer) => clearTimeout(timer),
};
const FRAME_LIMIT = 64 * 1024;
const QUEUE_LIMIT = 256 * 1024;

/** A new attachment owns one state machine. Output callbacks cannot revive a closed instance. */
export class TerminalSession {
  private wire: TerminalWire;
  private output: TerminalOutput;
  private notify: (status: TerminalStatus, message?: string) => void;
  private clock: TerminalClock;
  private state: TerminalStatus = "connecting";
  private stopped = false;
  private replay = false;
  private readyReceived = false;
  private sequence = 0;
  private deadline = 0;
  private timer?: ReturnType<typeof setTimeout>;
  private queue: Array<{ bytes: Uint8Array; historical: boolean }> = [];
  private queuedBytes = 0;
  private historicalBytes = 0;
  private writing = false;
  constructor(
    wire: TerminalWire,
    output: TerminalOutput,
    notify: (status: TerminalStatus, message?: string) => void,
    time: TerminalClock = clock,
  ) {
    this.wire = wire;
    this.output = output;
    this.notify = notify;
    this.clock = time;
    output.input(false);
    this.timer = time.timer(
      () => this.close("The terminal did not establish an attachment in time."),
      10_000,
    );
  }
  private valid() {
    if (this.stopped) return false;
    if (this.readyReceived && this.clock.monotonic() >= this.deadline) {
      this.close("The terminal access lease expired. Connect again to request current access.");
      return false;
    }
    return true;
  }
  private lease(expiresAt: string) {
    const remaining = Date.parse(expiresAt) - this.clock.wall();
    if (!Number.isFinite(remaining) || remaining <= 1000 || remaining > 31_000)
      throw new Error("Invalid lease");
    this.deadline = this.clock.monotonic() + remaining - 1000;
    if (this.timer !== undefined) this.clock.clear(this.timer);
    this.timer = this.clock.timer(
      () =>
        this.close("The terminal access lease expired. Connect again to request current access."),
      remaining - 1000,
    );
  }
  receive(data: string | ArrayBuffer) {
    if (!this.valid()) return;
    try {
      if (typeof data === "string") {
        if (new TextEncoder().encode(data).length > 1024) throw new Error("Oversize control");
        const control = parseComputer("terminal_server_control", JSON.parse(data));
        if (control.type === "ready") {
          if (this.readyReceived) throw new Error("Repeated ready");
          this.lease(control.expiresAt);
          this.readyReceived = true;
          this.state = "replaying";
          this.notify(this.state);
        } else if (control.type === "lease") {
          if (
            !this.readyReceived ||
            !Number.isSafeInteger(control.sequence) ||
            control.sequence <= this.sequence
          )
            throw new Error("Invalid lease sequence");
          this.lease(control.expiresAt);
          this.sequence = control.sequence;
        } else {
          if (!this.readyReceived || this.replay) throw new Error("Invalid replay boundary");
          this.replay = true;
          this.enableInput();
        }
      } else {
        if (
          !this.readyReceived ||
          data.byteLength > FRAME_LIMIT ||
          this.queuedBytes + data.byteLength > QUEUE_LIMIT ||
          this.queue.length >= 32
        )
          throw new Error("Output limit");
        if (data.byteLength === 0) return;
        const bytes = new Uint8Array(data);
        this.queue.push({ bytes, historical: !this.replay });
        this.queuedBytes += bytes.length;
        if (!this.replay) this.historicalBytes += bytes.length;
        this.drain();
      }
    } catch {
      this.close(
        "The terminal stream could not be verified or exceeded its buffer limit. Connect again.",
      );
    }
  }
  private drain() {
    if (this.writing || !this.valid()) return;
    const next = this.queue.shift();
    if (!next) {
      this.enableInput();
      return;
    }
    this.writing = true;
    try {
      this.output.write(next.bytes, () => {
        if (this.stopped) return;
        this.queuedBytes -= next.bytes.length;
        this.writing = false;
        if (next.historical) this.historicalBytes -= next.bytes.length;
        this.enableInput();
        this.drain();
      });
    } catch {
      this.close("Terminal rendering failed. Reopen the attachment after restoring the renderer.");
    }
  }
  private enableInput() {
    if (this.valid() && this.replay && this.historicalBytes === 0 && this.state !== "ready") {
      this.state = "ready";
      this.output.input(true);
      this.notify("ready");
    }
  }
  input(text: string, binary = false) {
    if (!this.valid() || this.state !== "ready") return;
    if (text.length > FRAME_LIMIT) {
      this.close(
        "The paste exceeded the terminal input limit. Use a file transfer for larger content.",
      );
      return;
    }
    const bytes = binary
      ? Uint8Array.from(text, (character) => character.charCodeAt(0))
      : new TextEncoder().encode(text);
    if (bytes.length > FRAME_LIMIT || this.wire.bufferedAmount + bytes.length > 128 * 1024) {
      this.close(
        "Terminal input is congested. Unsent input was discarded; check the running process before retrying a command.",
      );
      return;
    }
    try {
      this.wire.send(bytes);
    } catch {
      this.close(
        "The connection was interrupted. Check the running process before retrying a command.",
      );
    }
  }
  resize(cols: number, rows: number) {
    if (!this.valid() || this.state !== "ready") return;
    try {
      const control = parseComputer("terminal_resize", { type: "resize", cols, rows });
      const message = JSON.stringify(control);
      if (this.wire.bufferedAmount + message.length > 128 * 1024)
        throw new Error("Congested resize");
      this.wire.send(message);
    } catch {
      this.close("The terminal resize could not be sent.");
    }
  }
  close(message = "Disconnected. The Computer and its processes keep running.") {
    if (this.stopped) return;
    this.stopped = true;
    this.state = "disconnected";
    if (this.timer !== undefined) this.clock.clear(this.timer);
    this.queue = [];
    this.queuedBytes = 0;
    this.output.input(false);
    this.wire.close();
    this.notify("disconnected", message);
  }
}
