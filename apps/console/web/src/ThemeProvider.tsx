import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  applyTheme,
  consoleThemes,
  storedTheme,
  ThemeContext,
  type ConsoleTheme,
  type ThemeContextValue,
} from "./theme";

const initialTheme = storedTheme();
applyTheme(initialTheme);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<ConsoleTheme>(initialTheme);

  useEffect(() => applyTheme(theme), [theme]);

  const value = useMemo<ThemeContextValue>(() => {
    const definition = consoleThemes.find((candidate) => candidate.id === theme)!;
    return { theme, appTheme: definition.appTheme, setTheme };
  }, [theme]);

  return <ThemeContext value={value}>{children}</ThemeContext>;
}
