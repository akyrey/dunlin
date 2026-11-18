import { type ResolvedTheme, resolveTheme, useApplyResolvedTheme, usePrefersDark } from "@roost/ui";

import { useSettings } from "./settingsStore";

export { type ResolvedTheme, resolveTheme } from "@roost/ui";

/** The theme in effect, following the system when the setting says so. */
export function useResolvedTheme(): ResolvedTheme {
  const theme = useSettings((s) => s.settings.theme);
  return resolveTheme(theme, usePrefersDark());
}

/** Puts the theme in effect on the document (task 8.8). */
export function useApplyTheme(): void {
  useApplyResolvedTheme(useResolvedTheme());
}
