import type { Extension } from "@codemirror/state";
import { useMemo } from "react";

import { useSettings } from "../settings/settingsStore";
import { useResolvedTheme } from "../settings/theme";
import { themeExtension } from "./theme";

/** The CodeMirror theme for the current settings; a new value whenever they change. */
export function useEditorAppearance(): Extension {
  const theme = useResolvedTheme();
  const family = useSettings((s) => s.settings.editorFontFamily);
  const size = useSettings((s) => s.settings.editorFontSize);
  return useMemo(() => themeExtension(theme, { family, size }), [theme, family, size]);
}
