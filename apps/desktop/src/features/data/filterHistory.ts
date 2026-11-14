/** Recent filters per table, kept in localStorage (task 6.7). Only expressions the user typed. */
const PREFIX = "dunlin.filters.";
const KEEP = 10;

export function loadFilters(key: string, storage: Storage = localStorage): string[] {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(PREFIX + key) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((f): f is string => typeof f === "string") : [];
  } catch {
    // Unreadable history only means no suggestions.
    return [];
  }
}

/** Puts `filter` first, without duplicates, and keeps the last ten. */
export function rememberFilter(
  key: string,
  filter: string,
  storage: Storage = localStorage,
): string[] {
  const trimmed = filter.trim();
  const next = trimmed
    ? [trimmed, ...loadFilters(key, storage).filter((f) => f !== trimmed)].slice(0, KEEP)
    : loadFilters(key, storage);
  try {
    storage.setItem(PREFIX + key, JSON.stringify(next));
  } catch {
    // Storage full or disabled: the suggestions are just not saved.
  }
  return next;
}
