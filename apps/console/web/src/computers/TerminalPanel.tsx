import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { terminalTicket, computerError } from "./api";
import { parseComputer } from "../generatedContracts";
import { TerminalSession, type TerminalStatus } from "./terminalSession";
import "@xterm/xterm/css/xterm.css";

function hardwareRenderer(host: HTMLElement): boolean {
  for (const canvas of host.querySelectorAll("canvas")) {
    const gl = canvas.getContext("webgl2");
    if (!gl || gl.isContextLost()) continue;
    const debug = gl.getExtension("WEBGL_debug_renderer_info");
    const renderer = debug ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL)) : "";
    if (
      renderer &&
      /NVIDIA|AMD|ATI|Intel|Apple|Adreno|Mali|Radeon|GeForce|Quadro|Arc\b/i.test(renderer) &&
      !/SwiftShader|llvmpipe|softpipe|software|lavapipe|Microsoft Basic|WARP/i.test(renderer)
    )
      return true;
  }
  return false;
}

export function TerminalPanel({
  computerId,
  canConnect,
}: {
  computerId: string;
  canConnect: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | undefined>(undefined);
  const session = useRef<TerminalSession | undefined>(undefined);
  const disconnect = useRef<((message?: string) => void) | undefined>(undefined);
  const allowed = useRef(canConnect);
  const [attempt, setAttempt] = useState(0);
  const [status, setStatus] = useState<TerminalStatus>("disconnected");
  const [message, setMessage] = useState("Connect to continue work in this Computer.");
  useEffect(() => {
    allowed.current = canConnect;
    if (!canConnect)
      disconnect.current?.("Current Computer state or access does not permit an attachment.");
  }, [canConnect]);
  useEffect(() => {
    if (!attempt || !host.current || !allowed.current) return;
    const container = host.current;
    const abort = new AbortController();
    let ws: WebSocket | undefined;
    let relay: TerminalSession | undefined;
    let observer: ResizeObserver | undefined;
    const term = new Terminal({
      allowProposedApi: true,
      disableStdin: true,
      cursorBlink: true,
      scrollback: 2000,
      screenReaderMode: true,
      fontFamily: '"Geist Mono", ui-monospace, monospace',
      fontSize: 14,
      windowOptions: {},
      linkHandler: null,
      theme: { background: "#111113", foreground: "#e7e5df" },
    });
    terminal.current = term;
    // Output is untrusted: OSC clipboard writes and hyperlinks never reach the host browser.
    term.parser.registerOscHandler(52, () => true);
    term.parser.registerOscHandler(8, () => true);
    const notify = (next: TerminalStatus, reason?: string) => {
      if (abort.signal.aborted) return;
      setStatus(next);
      setMessage(
        reason ??
          (next === "ready"
            ? "Connected. Input is ready."
            : next === "replaying"
              ? "Restoring bounded terminal history…"
              : "Connecting…"),
      );
    };
    disconnect.current = (
      reason = "Disconnected. The Computer and its processes keep running.",
    ) => {
      notify("disconnected", reason);
      abort.abort();
      relay?.close();
      ws?.close();
      observer?.disconnect();
    };
    const fit = new FitAddon();
    const gpu = new WebglAddon();
    const fitBounded = () => {
      fit.fit();
      const cols = Math.min(500, Math.max(2, term.cols));
      const rows = Math.min(200, Math.max(1, term.rows));
      if (cols !== term.cols || rows !== term.rows) term.resize(cols, rows);
    };
    const run = async () => {
      try {
        term.loadAddon(fit);
        term.open(container);
        term.loadAddon(gpu);
        if (!hardwareRenderer(container))
          throw new Error("Hardware terminal rendering is unavailable in this browser.");
        gpu.onContextLoss(() => {
          notify(
            "disconnected",
            "Hardware graphics was lost. Reopen the terminal after restoring GPU access.",
          );
          relay?.close(
            "Hardware graphics was lost. Reopen the terminal after restoring GPU access.",
          );
          abort.abort();
          ws?.close();
          term.dispose();
        });
        fitBounded();
        notify("connecting");
        const ticket = await terminalTicket(computerId, abort.signal);
        if (abort.signal.aborted || !allowed.current) return;
        const url = new URL(ticket.endpoint, window.location.origin);
        url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
        ws = new WebSocket(url);
        ws.binaryType = "arraybuffer";
        const socket = ws;
        relay = new TerminalSession(
          {
            get bufferedAmount() {
              return socket.bufferedAmount;
            },
            send: (data) => socket.send(data),
            close: () => socket.close(),
          },
          {
            write: (bytes, done) => term.write(bytes, done),
            input: (enabled) => {
              term.options.disableStdin = !enabled;
            },
          },
          notify,
        );
        session.current = relay;
        const current = relay;
        socket.onopen = () => {
          if (abort.signal.aborted) {
            socket.close();
            return;
          }
          try {
            const attach = parseComputer("terminal_attach", {
              type: "attach",
              version: 2,
              computerId,
              token: ticket.token,
              cols: Math.min(500, Math.max(2, term.cols)),
              rows: Math.min(200, Math.max(1, term.rows)),
            });
            const frame = JSON.stringify(attach);
            if (new TextEncoder().encode(frame).length > 1024)
              throw new Error("Oversize attachment");
            socket.send(frame);
            ticket.token = "";
          } catch {
            current.close("The terminal attachment could not be established.");
          }
        };
        socket.onmessage = (event: MessageEvent<unknown>) => {
          if (abort.signal.aborted) return;
          if (typeof event.data === "string" || event.data instanceof ArrayBuffer)
            current.receive(event.data);
          else current.close("The terminal returned an unexpected frame.");
        };
        socket.onerror = () =>
          current.close(
            "The connection was interrupted. Connect again and inspect the process before repeating a command.",
          );
        socket.onclose = () => {
          ticket.token = "";
          current.close(
            "The attachment ended. Connect again to request current access. The Computer may still be running.",
          );
        };
        term.onData((data) => current.input(data));
        term.onBinary((data) => current.input(data, true));
        term.onResize(({ cols, rows }) =>
          current.resize(Math.min(500, Math.max(2, cols)), Math.min(200, Math.max(1, rows))),
        );
        observer = new ResizeObserver(() => {
          if (!abort.signal.aborted) fitBounded();
        });
        observer.observe(container);
        term.focus();
      } catch (error) {
        if (!abort.signal.aborted) {
          notify(
            "disconnected",
            error instanceof Error && error.message.startsWith("Hardware terminal")
              ? error.message
              : computerError(error),
          );
          term.dispose();
        }
      }
    };
    void run();
    return () => {
      abort.abort();
      observer?.disconnect();
      relay?.close();
      ws?.close();
      term.dispose();
      terminal.current = undefined;
      session.current = undefined;
      disconnect.current = undefined;
    };
  }, [computerId, attempt]);
  const active = status !== "disconnected";
  return (
    <section className="computer-terminal" aria-label="Terminal">
      <div className="computers-toolbar">
        <p role="status">{message}</p>
        <div className="computers-actions">
          <button
            className="button button-secondary"
            disabled={!attempt}
            onClick={async () => {
              const text = terminal.current?.getSelection();
              if (!text) {
                setMessage("Select terminal text to copy.");
                return;
              }
              try {
                await navigator.clipboard.writeText(text);
                setMessage("Selection copied.");
              } catch {
                setMessage("Clipboard access was denied. Use your browser’s copy shortcut.");
              }
            }}
          >
            Copy selection
          </button>
          {active ? (
            <button className="button button-secondary" onClick={() => disconnect.current?.()}>
              Disconnect
            </button>
          ) : (
            <button
              className="button button-primary"
              disabled={!canConnect}
              onClick={() => setAttempt((value) => value + 1)}
            >
              Connect
            </button>
          )}
        </div>
      </div>
      <div className="computer-terminal-screen" ref={host} />
      <p className="computer-terminal-note">
        History is limited to the retained replay window. Disconnect keeps the Computer running.
        Unsent input is discarded on interruption.
      </p>
    </section>
  );
}
