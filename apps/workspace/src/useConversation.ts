import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError } from "./api.ts";
import { mergeMessages } from "./conversation.ts";
import type { ConversationSnapshot } from "./api.ts";

export function useConversation(chat: string, changed: () => void) {
  const [snapshot, setSnapshot] = useState<ConversationSnapshot>();
  const [error, setError] = useState<string>();
  const [connected, setConnected] = useState(false);
  const [hasOlder, setHasOlder] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const current = useRef<ConversationSnapshot | undefined>(undefined);
  const pending = useRef<Promise<void> | undefined>(undefined);
  const rerun = useRef(false);
  const controller = useRef(new AbortController());
  const changedRef = useRef(changed);
  changedRef.current = changed;
  const commit = useCallback((value: ConversationSnapshot) => { current.current = value; setSnapshot(value); }, []);
  const refresh = useCallback(async () => {
    if (pending.current) { rerun.current = true; return pending.current; }
    const signal = controller.current.signal;
    pending.current = (async () => {
      do {
        rerun.current = false;
        const previous = current.current;
        if (!previous) {
          const first = await api.snapshot(chat, {}, signal);
          if (signal.aborted) return;
          setHasOlder(first.messages.length === 100); commit(first);
        } else {
          let after = previous.chat.sequence;
          let messages = previous.messages;
          // Each response is bounded. Large catch-up yields between pages and
          // carries the last actual message sequence, never skips to a head.
          for (;;) {
            const page = await api.snapshot(chat, { after }, signal);
            if (signal.aborted) return;
            messages = mergeMessages(messages, page.messages);
            if (messages.length > 1000) { messages = messages.slice(-1000); setHasOlder(true); }
            if (page.messages.length < 100) { commit({ ...page, messages }); break; }
            after = page.messages[page.messages.length - 1].sequence;
          }
        }
        setError(undefined); changedRef.current();
      } while (rerun.current && !signal.aborted);
    })().catch((error: unknown) => {
      if (signal.aborted) return;
      if (error instanceof ApiError && [401, 403, 404].includes(error.status)) {
        current.current = undefined; setSnapshot(undefined);
      }
      setError(error instanceof Error ? error.message : "Could not load the conversation.");
    }).finally(() => { pending.current = undefined; });
    return pending.current;
  }, [chat, commit]);
  useEffect(() => {
    controller.current = new AbortController();
    const source = new EventSource(`/workspace/api/chats/${encodeURIComponent(chat)}/events`);
    source.onopen = () => { setConnected(true); void refresh(); };
    source.addEventListener("change", () => { void refresh(); });
    // Expiry ends this watch, not the conversation. EventSource reconnects at
    // the server's retry interval and the new request rechecks authority. A
    // denied snapshot clears history; a temporary check failure can recover.
    source.addEventListener("expired", () => { setConnected(false); void refresh(); });
    source.onerror = () => { setConnected(false); void refresh(); };
    const focused = () => { if (document.visibilityState === "visible") void refresh(); };
    document.addEventListener("visibilitychange", focused);
    window.addEventListener("focus", focused);
    void refresh();
    return () => {
      source.close(); controller.current.abort();
      document.removeEventListener("visibilitychange", focused);
      window.removeEventListener("focus", focused);
    };
  }, [chat, refresh]);
  const older = useCallback(async () => {
    const value = current.current;
    if (!value || loadingOlder || !value.messages.length) return;
    setLoadingOlder(true);
    try {
      const page = await api.snapshot(chat, { before: value.messages[0].sequence }, controller.current.signal);
      const latest = current.current;
      if (!latest || controller.current.signal.aborted) return;
      commit({ ...latest, messages: mergeMessages(page.messages, latest.messages) });
      setHasOlder(page.messages.length === 100);
    } catch (error) { if (!controller.current.signal.aborted) setError(error instanceof Error ? error.message : "Could not load history."); }
    finally { setLoadingOlder(false); }
  }, [chat, commit, loadingOlder]);
  return { snapshot, error, connected, refresh, older, hasOlder, loadingOlder };
}
