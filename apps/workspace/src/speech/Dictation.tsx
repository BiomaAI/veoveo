import { useEffect, useRef, useState } from "react";
import { Mic, Square, X } from "lucide-react";
import { Capture } from "./capture.ts";

export function Dictation({ disabled, onText, onActive }: { disabled: boolean; onText: (text: string) => void; onActive: (active: boolean) => void }) {
  const [phase, setPhase] = useState<"idle" | "starting" | "listening" | "finishing" | "failed">("idle");
  const [partial, setPartial] = useState("");
  const [error, setError] = useState("");
  const current = useRef<Capture | undefined>(undefined);
  const callbacks = useRef({ onText, onActive }); callbacks.current = { onText, onActive };
  const active = phase === "starting" || phase === "listening" || phase === "finishing";
  const stop = async () => {
    const capture = current.current;
    if (!capture) return;
    setPhase("finishing");
    try {
      const text = await capture.finish();
      if (current.current !== capture) return;
      callbacks.current.onText(text); setPartial(""); setPhase("idle");
    } catch (error) {
      if (current.current !== capture) return;
      setError(error instanceof Error ? error.message : "Dictation couldn't finish. Your draft is preserved."); setPhase("failed");
    } finally {
      if (current.current === capture) { current.current = undefined; callbacks.current.onActive(false); }
    }
  };
  const cancel = () => {
    const capture = current.current; current.current = undefined;
    void capture?.cancel(); callbacks.current.onActive(false);
    setPhase("idle"); setPartial(""); setError("");
  };
  useEffect(() => {
    const leave = () => { void current.current?.cancel(); current.current = undefined; callbacks.current.onActive(false); };
    const lost = () => { leave(); setPhase("failed"); setError("Sign in again to use the microphone. Your draft is preserved."); };
    window.addEventListener("pagehide", leave); window.addEventListener("workspace-auth-expired", lost);
    return () => { leave(); window.removeEventListener("pagehide", leave); window.removeEventListener("workspace-auth-expired", lost); };
  }, []);
  useEffect(() => { if (disabled && current.current) cancel(); }, [disabled]);
  const start = async () => {
    setError(""); setPartial(""); setPhase("starting"); callbacks.current.onActive(true);
    const capture = new Capture({ partial: text => { if (current.current === capture) setPartial(text); },
      failed: message => { if (current.current !== capture) return; current.current = undefined; setError(message); setPhase("failed"); callbacks.current.onActive(false); },
      limit: () => { if (current.current === capture) void stop(); } });
    current.current = capture;
    try { await capture.start(); if (current.current === capture) setPhase("listening"); }
    catch (error) { await capture.cancel(); if (current.current !== capture) return; current.current = undefined; callbacks.current.onActive(false); setPhase("failed"); setError(error instanceof Error ? error.message : "Veoveo couldn't use the microphone. Check that this site is allowed to use it in your browser settings."); }
  };
  return <div className="dictation" aria-label="Private dictation">
    <div className="dictation-controls">
      {!active && <button type="button" disabled={disabled} onClick={() => void start()}><Mic size={15}/> Dictate</button>}
      {phase === "listening" && <button type="button" onClick={() => void stop()}><Square size={13}/> Stop dictation</button>}
      {active && <button type="button" onClick={cancel}><X size={14}/> Cancel</button>}
      <span role="status">{phase === "starting" ? "Opening microphone…" : phase === "listening" ? "Listening · private draft · up to 2 minutes" : phase === "finishing" ? "Finishing transcript…" : "Review your words before sending."}</span>
    </div>
    {active && <p className="dictation-preview" aria-live="polite">{partial || "Text appears after a few seconds of speech."}</p>}
    {error && <p className="error" role="alert">{error}</p>}
    {phase === "failed" && partial && <div className="dictation-preview"><p>{partial}</p><button type="button" disabled={disabled} onClick={() => { onText(partial); setPartial(""); }}>Add available text to draft</button><button type="button" onClick={() => setPartial("")}>Discard</button></div>}
  </div>;
}
