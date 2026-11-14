import type { Settings } from "../../ipc";
import { BOUNDS } from "./defaults";

export type NumberField = "editorFontSize" | "defaultRowLimit" | "defaultPageSize" | "historyCap";

export const NUMBER_LABELS: Record<NumberField, string> = {
  editorFontSize: "Editor font size",
  defaultRowLimit: "Default row limit",
  defaultPageSize: "Default page size",
  historyCap: "History size",
};

/** The first problem with `draft`, checked again by the backend. */
export function settingsProblem(draft: Settings): string | null {
  for (const field of Object.keys(NUMBER_LABELS) as NumberField[]) {
    const { min, max } = BOUNDS[field];
    const value = draft[field];
    if (!Number.isInteger(value) || value < min || value > max)
      return `${NUMBER_LABELS[field]} must be a whole number between ${min} and ${max}.`;
  }
  if (!draft.editorFontFamily.trim()) return "Name an editor font.";
  if (Array.from(draft.nullText).length > BOUNDS.nullTextChars)
    return `The NULL text can have at most ${BOUNDS.nullTextChars} characters.`;
  return null;
}
