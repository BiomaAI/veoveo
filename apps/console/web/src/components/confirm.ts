import { createContext, useContext, type ReactNode } from "react";

export interface ConfirmOptions {
  title: string;
  body: ReactNode;
  confirmLabel?: string;
  tone?: "default" | "danger";
}

export type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

export const ConfirmContext = createContext<ConfirmFn>(() => Promise.resolve(false));

export function useConfirm(): ConfirmFn {
  return useContext(ConfirmContext);
}
