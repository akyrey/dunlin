import { useEffect, useState } from "react";

import { Button } from "../../components/Button";
import { Skeleton } from "../../components/Skeleton";
import { type AppError, type KeyContentIpc, kvRead, toAppError } from "../../ipc";
import { ResultView } from "../grid/ResultView";
import { formatTtl } from "./ttl";

type Props = { sessionId: string; id: string; name: string };

/** One key: its type, TTL, size and content in the result grid (D-032). */
export function KeyView({ sessionId, id, name }: Props) {
  const [content, setContent] = useState<KeyContentIpc | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  /** Bumped by Refresh; each value is one read. */
  const [revision, setRevision] = useState(0);
  const [loaded, setLoaded] = useState(-1);
  const loading = loaded !== revision;

  useEffect(() => {
    let stale = false;
    kvRead(sessionId, id).then(
      (read) => {
        if (stale) return;
        setContent(read);
        setError(null);
        setLoaded(revision);
      },
      (caught: unknown) => {
        if (stale) return;
        setError(toAppError(caught));
        setLoaded(revision);
      },
    );
    return () => {
      stale = true;
    };
  }, [sessionId, id, revision]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-zinc-800 px-3 py-1.5 text-xs">
        <span className="truncate font-mono text-sm text-zinc-100" title={name}>
          {name}
        </span>
        {content && (
          <>
            <span className="rounded bg-zinc-800 px-1.5 py-0.5 font-mono uppercase text-zinc-300">
              {content.keyType}
            </span>
            <span className="text-zinc-400" title="Time to live">
              TTL {formatTtl(content.ttlMs)}
            </span>
            <span className="text-zinc-400">
              {content.keyType === "string"
                ? `${content.length.toLocaleString()} bytes`
                : `${content.length.toLocaleString()} ${content.length === 1 ? "element" : "elements"}`}
            </span>
          </>
        )}
        <Button
          variant="ghost"
          className="ml-auto py-0.5 text-xs"
          disabled={loading}
          onClick={() => {
            setRevision((n) => n + 1);
          }}
        >
          Refresh
        </Button>
      </div>
      {error && (
        <p role="alert" className="px-3 py-2 text-sm text-rose-300">
          {error.message}
        </p>
      )}
      {content?.truncated && (
        <p
          role="status"
          className="border-b border-amber-900 bg-amber-950/60 px-3 py-1 text-xs text-amber-300"
        >
          Showing the first {content.rows.length.toLocaleString()} of{" "}
          {content.length.toLocaleString()}
          {content.keyType === "string" ? " bytes" : " elements"}.
        </p>
      )}
      <div className="min-h-0 flex-1">
        {loading && !content ? (
          <Skeleton rows={6} label="Loading the key" />
        ) : content && content.columns.length > 0 ? (
          <ResultView
            columns={content.columns}
            rows={content.rows}
            resetKey={content}
            loadFull={null}
            noFullReason="Long values are shown up to the display limit; read the rest with GETRANGE in the console."
            exportSource={{ sessionId, name, columns: content.columns }}
          />
        ) : (
          content && (
            <p className="p-4 text-sm text-zinc-500">
              Values of type {content.keyType} are not shown; use the console.
            </p>
          )
        )}
      </div>
    </div>
  );
}
