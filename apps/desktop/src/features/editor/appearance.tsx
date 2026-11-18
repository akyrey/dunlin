import { type EditorSettings, EditorSettingsProvider } from "@roost/ui";
import { type ReactNode, useMemo } from "react";

import { useSettings } from "../settings/settingsStore";
import { useResolvedTheme } from "../settings/theme";

/** Feeds every editor the theme and font from the settings (task 8.8). Loads no CodeMirror. */
export function EditorAppearance({ children }: { children: ReactNode }) {
  const theme = useResolvedTheme();
  const family = useSettings((s) => s.settings.editorFontFamily);
  const size = useSettings((s) => s.settings.editorFontSize);
  const settings = useMemo(
    (): EditorSettings => ({ theme, font: { family, size } }),
    [theme, family, size],
  );
  return <EditorSettingsProvider settings={settings}>{children}</EditorSettingsProvider>;
}
