import { useEffect, useState } from "react";

import type { Theme } from "../../ipc";
import { useSettings } from "./settingsStore";

export type ResolvedTheme = "dark" | "light";

const DARK_QUERY = "(prefers-color-scheme: dark)";

function systemDark(): boolean {
  return typeof window.matchMedia === "function" ? window.matchMedia(DARK_QUERY).matches : true;
}

export function resolveTheme(theme: Theme, prefersDark: boolean): ResolvedTheme {
  if (theme === "system") return prefersDark ? "dark" : "light";
  return theme;
}

/** The theme in effect, following the system when the setting says so. */
export function useResolvedTheme(): ResolvedTheme {
  const theme = useSettings((s) => s.settings.theme);
  const [prefersDark, setPrefersDark] = useState(systemDark);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia(DARK_QUERY);
    const changed = (event: MediaQueryListEvent) => {
      setPrefersDark(event.matches);
    };
    query.addEventListener("change", changed);
    return () => {
      query.removeEventListener("change", changed);
    };
  }, []);
  return resolveTheme(theme, prefersDark);
}

/** Puts the theme on `<html data-theme>`, which swaps the colour variables (theme-light.css). */
export function useApplyTheme(): void {
  const resolved = useResolvedTheme();
  useEffect(() => {
    document.documentElement.dataset.theme = resolved;
    document.documentElement.style.colorScheme = resolved;
  }, [resolved]);
}
