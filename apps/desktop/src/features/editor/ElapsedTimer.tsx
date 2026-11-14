import { useEffect, useState } from "react";

const TICK_MS = 100;

/** Seconds since `startedAt`, updated while mounted. */
export function ElapsedTimer({ startedAt }: { startedAt: number }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => {
      setNow(Date.now());
    }, TICK_MS);
    return () => {
      window.clearInterval(id);
    };
  }, []);
  const seconds = Math.max(0, now - startedAt) / 1000;
  return (
    <span role="timer" className="font-mono text-xs tabular-nums text-zinc-400">
      {seconds.toFixed(1)} s
    </span>
  );
}
