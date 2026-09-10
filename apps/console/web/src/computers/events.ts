import { consumeServerSentEvents } from "../apps/resourceEventStream.ts";
import { acceptConsoleCsrfToken, consoleCsrfToken } from "../csrf.ts";
import { authenticationRequired } from "../auth.ts";
import { parseComputer } from "../generatedContracts.ts";

export type LiveState = "connecting" | "live" | "reconnecting" | "denied";
export function watchComputers(
  changed: () => void,
  status: (state: LiveState) => void,
): () => void {
  const stop = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let delay = 500;
  const connect = async () => {
    const attempt = new AbortController();
    const admission = setTimeout(() => attempt.abort(), 15_000);
    try {
      const csrf = consoleCsrfToken();
      if (!csrf) throw new Error("Session unavailable");
      const response = await fetch("/console/api/computers/events", {
        method: "POST",
        credentials: "same-origin",
        cache: "no-store",
        redirect: "error",
        headers: {
          Accept: "text/event-stream",
          "Content-Type": "application/json",
          "X-Veoveo-CSRF-Token": csrf,
        },
        body: "{}",
        signal: AbortSignal.any([stop.signal, attempt.signal]),
      });
      clearTimeout(admission);
      acceptConsoleCsrfToken(response.headers.get("x-veoveo-csrf-token"));
      if (response.status === 401) {
        stop.abort();
        authenticationRequired();
      }
      if (response.status === 403) {
        stop.abort();
        status("denied");
        return;
      }
      if (
        !response.ok ||
        !response.body ||
        response.headers.get("content-type")?.split(";")[0] !== "text/event-stream"
      ) {
        await response.body?.cancel();
        throw new Error("Computer updates unavailable");
      }
      await consumeServerSentEvents(
        response.body,
        (event) => {
          if (stop.signal.aborted) return;
          if (event.type !== "computer") throw new Error("Unexpected Computer event");
          parseComputer("event", JSON.parse(event.data));
          delay = 500;
          status("live");
          changed();
        },
        { maxEventBytes: 4096, idleMilliseconds: 25_000 },
      );
    } catch {
      /* A fresh subscription establishes a new baseline after transport loss. */
    } finally {
      clearTimeout(admission);
      attempt.abort();
    }
    if (!stop.signal.aborted) {
      status("reconnecting");
      timer = setTimeout(() => void connect(), delay);
      delay = Math.min(delay * 2, 10_000);
    }
  };
  status("connecting");
  void connect();
  return () => {
    stop.abort();
    if (timer !== undefined) clearTimeout(timer);
  };
}
