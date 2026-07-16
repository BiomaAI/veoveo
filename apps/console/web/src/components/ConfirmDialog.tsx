import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ConfirmContext, type ConfirmFn, type ConfirmOptions } from "./confirm";

interface PendingConfirm {
  options: ConfirmOptions;
  resolve: (value: boolean) => void;
}

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<PendingConfirm>();
  const cancelRef = useRef<HTMLButtonElement>(null);

  const confirm = useCallback<ConfirmFn>(
    (options) =>
      new Promise<boolean>((resolve) => {
        setPending({ options, resolve });
      }),
    []
  );

  const settle = (value: boolean) => {
    pending?.resolve(value);
    setPending(undefined);
  };

  useEffect(() => {
    if (!pending) return;
    cancelRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        pending.resolve(false);
        setPending(undefined);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [pending]);

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      {pending && (
        <div className="modal-layer">
          <button className="drawer-scrim" aria-label="Cancel" onClick={() => settle(false)} />
          <div className="modal" role="alertdialog" aria-modal="true" aria-label={pending.options.title}>
            <h2>{pending.options.title}</h2>
            <div className="modal-body">{pending.options.body}</div>
            <div className="modal-actions">
              <button ref={cancelRef} className="button button-secondary" onClick={() => settle(false)}>
                Cancel
              </button>
              <button
                className={`button ${pending.options.tone === "danger" ? "button-danger" : "button-primary"}`}
                onClick={() => settle(true)}
              >
                {pending.options.confirmLabel ?? "Confirm"}
              </button>
            </div>
          </div>
        </div>
      )}
    </ConfirmContext.Provider>
  );
}
