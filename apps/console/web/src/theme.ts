import { createContext, useContext } from "react";

export const consoleThemes = [
  { id: "rerun-dark", label: "Rerun Dark", appTheme: "dark" },
  { id: "light", label: "Light", appTheme: "light" },
] as const;

export type ConsoleTheme = (typeof consoleThemes)[number]["id"];
export type AppTheme = (typeof consoleThemes)[number]["appTheme"];

const STORAGE_KEY = "veoveo.console.theme";
const DEFAULT_THEME: ConsoleTheme = "rerun-dark";

export interface ThemeContextValue {
  theme: ConsoleTheme;
  appTheme: AppTheme;
  setTheme: (theme: ConsoleTheme) => void;
}

export const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

function isConsoleTheme(value: string | null): value is ConsoleTheme {
  return consoleThemes.some((theme) => theme.id === value);
}

export function storedTheme(): ConsoleTheme {
  const value = window.localStorage.getItem(STORAGE_KEY);
  return isConsoleTheme(value) ? value : DEFAULT_THEME;
}

export function applyTheme(theme: ConsoleTheme) {
  const definition = consoleThemes.find((candidate) => candidate.id === theme)!;
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = definition.appTheme;
  window.localStorage.setItem(STORAGE_KEY, theme);
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used inside ThemeProvider");
  return value;
}
