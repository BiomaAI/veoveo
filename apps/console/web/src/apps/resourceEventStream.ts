export interface ServerSentEvent {
  type: string;
  data: string;
}

export interface ResourceEventStream {
  close: () => void;
}

export interface ResourceEventStreamHandlers {
  onOpen: () => void;
  onEvent: (event: ServerSentEvent) => void;
  onInitialError: (error: Error) => void;
}

type FetchResource = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

const INITIAL_RETRY_MILLISECONDS = 250;
const MAXIMUM_RETRY_MILLISECONDS = 5_000;

function errorValue(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

/** Decode one fetch-backed `text/event-stream` response. */
export async function consumeServerSentEvents(
  body: ReadableStream<Uint8Array>,
  emit: (event: ServerSentEvent) => void,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let type = "message";
  let data: string[] = [];

  const consumeLine = (value: string) => {
    if (value.length === 0) {
      if (data.length > 0) emit({ type, data: data.join("\n") });
      type = "message";
      data = [];
      return;
    }
    if (value.startsWith(":")) return;
    const separator = value.indexOf(":");
    const field = separator < 0 ? value : value.slice(0, separator);
    const raw = separator < 0 ? "" : value.slice(separator + 1);
    const fieldValue = raw.startsWith(" ") ? raw.slice(1) : raw;
    if (field === "event") type = fieldValue;
    if (field === "data") data.push(fieldValue);
  };

  try {
    while (true) {
      const { value, done } = await reader.read();
      buffer += decoder.decode(value, { stream: !done });
      let newline = buffer.indexOf("\n");
      while (newline >= 0) {
        const rawLine = buffer.slice(0, newline);
        buffer = buffer.slice(newline + 1);
        consumeLine(rawLine.endsWith("\r") ? rawLine.slice(0, -1) : rawLine);
        newline = buffer.indexOf("\n");
      }
      if (!done) continue;
      if (buffer.length > 0) consumeLine(buffer.endsWith("\r") ? buffer.slice(0, -1) : buffer);
      consumeLine("");
      return;
    }
  } finally {
    reader.releaseLock();
  }
}

/**
 * Open one abortable fetch-backed wake stream. Reconnection is triggered only
 * by a stream failure or close, with bounded backoff; healthy operation has
 * no polling loop and no periodic browser timer.
 */
export function openResourceEventStream(
  url: string,
  handlers: ResourceEventStreamHandlers,
  fetchResource: FetchResource = fetch,
): ResourceEventStream {
  let closed = false;
  let opened = false;
  let controller: AbortController | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let finishRetry: (() => void) | undefined;

  const waitForRetry = (milliseconds: number) =>
    new Promise<void>((resolve) => {
      finishRetry = resolve;
      retryTimer = setTimeout(() => {
        retryTimer = undefined;
        finishRetry = undefined;
        resolve();
      }, milliseconds);
    });

  const run = async () => {
    let retryMilliseconds = INITIAL_RETRY_MILLISECONDS;
    while (!closed) {
      controller = new AbortController();
      try {
        const response = await fetchResource(url, {
          credentials: "same-origin",
          headers: { Accept: "text/event-stream" },
          cache: "no-store",
          signal: controller.signal,
        });
        if (!response.ok) throw new Error(`resource event stream returned ${response.status}`);
        if (!response.body) throw new Error("resource event stream returned no body");
        opened = true;
        retryMilliseconds = INITIAL_RETRY_MILLISECONDS;
        handlers.onOpen();
        await consumeServerSentEvents(response.body, handlers.onEvent);
        if (!closed) throw new Error("resource event stream ended");
      } catch (error) {
        if (closed || isAbort(error)) return;
        if (!opened) {
          handlers.onInitialError(errorValue(error));
          return;
        }
        await waitForRetry(retryMilliseconds);
        retryMilliseconds = Math.min(MAXIMUM_RETRY_MILLISECONDS, retryMilliseconds * 2);
      }
    }
  };

  void run().catch((error: unknown) => handlers.onInitialError(errorValue(error)));
  return {
    close: () => {
      if (closed) return;
      closed = true;
      controller?.abort();
      if (retryTimer !== undefined) clearTimeout(retryTimer);
      retryTimer = undefined;
      finishRetry?.();
      finishRetry = undefined;
    },
  };
}
