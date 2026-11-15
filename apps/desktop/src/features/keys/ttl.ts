/** "12 s", "3 min", "2 h", "5 d": the time left before the key expires. */
export function formatTtl(ms: number | null): string {
  if (ms === null) return "no expiry";
  const seconds = ms / 1000;
  if (seconds < 120) return `${Math.max(0, Math.round(seconds))} s`;
  if (seconds < 7200) return `${Math.round(seconds / 60)} min`;
  if (seconds < 172_800) return `${Math.round(seconds / 3600)} h`;
  return `${Math.round(seconds / 86_400)} d`;
}
