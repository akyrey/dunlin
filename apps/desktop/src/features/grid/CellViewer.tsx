import { useEffect, useMemo, useState } from "react";

import { Button, copyWithToast } from "@roost/ui";
import { type AppError, type Value, toAppError } from "../../ipc";
import { decodeBase64, hexDump, imageType } from "../../lib/binary";
import { prettyJson } from "../../lib/json";
import { formatSize, rawText } from "../../lib/value";
import { prettyXml } from "../../lib/xml";
import { type Parsed, editText } from "../edit/editors";
import { CodeEditor } from "../editor/CodeEditor";
import { type CodeLanguage, ReadOnlyCode } from "../editor/ReadOnlyCode";
import type { GridColumn } from "./DataGrid";

type Props = {
  column: GridColumn;
  value: Value;
  /** Fetches the whole value of a truncated cell; `null` when the row cannot be addressed. */
  loadFull: (() => Promise<Value>) | null;
  /** Why `loadFull` is unavailable. */
  noFullReason: string;
  onClose: () => void;
  /** Editing in the panel (JSON, XML, long text); absent for read-only cells. */
  edit?: ViewerEdit;
  /** Wide enough to read a large value; the parent owns it so it survives moving between cells. */
  expanded?: boolean;
  /** Absent: the panel cannot be expanded. */
  onExpandedChange?: (expanded: boolean) => void;
};

export type ViewerEdit = {
  initialText: string;
  parse: (text: string) => Parsed;
  onSave: (value: Value) => void;
  /** Open straight in edit mode. */
  start: boolean;
};

const COLLAPSED_WIDTH = "w-96";
/** Most of the result area, leaving a strip of the grid to keep its place. */
const EXPANDED_WIDTH = "w-[75%]";

function isTruncated(value: Value): boolean {
  return (
    (value.t === "text" || value.t === "json" || value.t === "xml" || value.t === "bytes") &&
    value.truncated === true
  );
}

/** The selected cell in full: JSON and XML pretty-printed, text with wrapping, binary as a hex
 * dump or an image (task 6.10). Nothing is ever rendered as HTML. */
export function CellViewer({
  column,
  value,
  loadFull,
  noFullReason,
  onClose,
  edit,
  expanded = false,
  onExpandedChange,
}: Props) {
  // A value cut short is never edited: saving it would cut the stored value too.
  const [draft, setDraft] = useState<string | null>(
    edit?.start && !isTruncated(value) ? edit.initialText : null,
  );
  const [invalid, setInvalid] = useState<string | null>(null);
  const [full, setFull] = useState<{ value: Value; for: Value } | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(false);
  const [wrap, setWrap] = useState(true);
  const shown = full?.for === value ? full.value : value;
  const truncated = isTruncated(shown);

  const load = async () => {
    if (!loadFull) return;
    setLoading(true);
    setError(null);
    try {
      setFull({ value: await loadFull(), for: value });
    } catch (caught) {
      setError(toAppError(caught));
    } finally {
      setLoading(false);
    }
  };

  return (
    <aside
      aria-label="Cell viewer"
      className={`flex h-full shrink-0 flex-col border-l border-zinc-800 bg-zinc-900 ${
        expanded ? EXPANDED_WIDTH : COLLAPSED_WIDTH
      }`}
    >
      <header className="flex items-center gap-2 border-b border-zinc-800 px-2 py-1.5 text-xs">
        <span className="min-w-0 flex-1 truncate">
          <span className="font-semibold text-zinc-200">{column.name}</span>{" "}
          <span className="text-zinc-500">{column.typeName}</span>
        </span>
        <label className="flex items-center gap-1 text-zinc-400">
          <input
            type="checkbox"
            checked={wrap}
            onChange={(e) => {
              setWrap(e.target.checked);
            }}
          />
          Wrap
        </label>
        <Button
          variant="ghost"
          className="px-1 text-xs"
          onClick={() => void copyWithToast(rawText(shown), "the value")}
        >
          Copy
        </Button>
        {edit && draft === null && (
          <Button
            variant="ghost"
            className="px-1 text-xs"
            disabled={truncated}
            title={truncated ? "Load the whole value before editing it" : undefined}
            onClick={() => {
              setDraft(full?.for === value ? editText(full.value) : edit.initialText);
            }}
          >
            Edit
          </Button>
        )}
        {onExpandedChange && (
          <Button
            variant="ghost"
            className="px-1 text-xs"
            aria-label={expanded ? "Collapse cell viewer" : "Expand cell viewer"}
            title={expanded ? "Collapse" : "Expand"}
            onClick={() => {
              onExpandedChange(!expanded);
            }}
          >
            {expanded ? "⇥" : "⇤"}
          </Button>
        )}
        <Button
          variant="ghost"
          className="px-1 text-xs"
          aria-label="Close cell viewer"
          onClick={onClose}
        >
          ×
        </Button>
      </header>
      {truncated && (
        <p
          role="status"
          className="flex items-center gap-2 border-b border-zinc-800 px-2 py-1 text-[11px] text-amber-300"
        >
          {loadFull
            ? "Showing the start of a long value."
            : `Showing the start of a long value. ${noFullReason}`}
          {loadFull && (
            <Button
              variant="ghost"
              className="px-1 text-[11px]"
              disabled={loading}
              onClick={() => void load()}
            >
              {loading ? "Loading…" : "Load the whole value"}
            </Button>
          )}
        </p>
      )}
      {error && (
        <p role="alert" className="border-b border-zinc-800 px-2 py-1 text-[11px] text-rose-300">
          {error.message}
        </p>
      )}
      <div className="min-h-0 flex-1">
        {edit && draft !== null ? (
          <div className="flex h-full flex-col gap-1 p-2">
            <CodeEditor
              initialText={draft}
              label={`Edit ${column.name}`}
              language={editLanguage(shown, column.category)}
              wrap={wrap}
              invalid={invalid !== null}
              onChange={(text) => {
                setDraft(text);
                setInvalid(null);
              }}
            />
            {invalid && (
              <p role="alert" className="text-[11px] text-rose-300">
                {invalid}
              </p>
            )}
            <div className="flex justify-end gap-2">
              <Button
                variant="ghost"
                className="text-xs"
                onClick={() => {
                  setDraft(null);
                  setInvalid(null);
                }}
              >
                Cancel
              </Button>
              <Button
                variant="primary"
                className="text-xs"
                onClick={() => {
                  const parsed = edit.parse(draft);
                  if ("error" in parsed) {
                    setInvalid(parsed.error);
                    return;
                  }
                  edit.onSave(parsed.value);
                  setDraft(null);
                }}
              >
                Set value
              </Button>
            </div>
          </div>
        ) : (
          <Body value={shown} category={column.category} wrap={wrap} />
        )}
      </div>
    </aside>
  );
}

function Body({
  value,
  category,
  wrap,
}: {
  value: Value;
  category: GridColumn["category"];
  wrap: boolean;
}) {
  const content = useMemo(() => describe(value, category), [value, category]);
  if (content.kind === "binary") return <Binary base64={content.base64} size={content.size} />;
  return (
    <div className="flex h-full flex-col">
      {content.note && <p className="px-2 py-1 text-[11px] text-zinc-500">{content.note}</p>}
      <div className="min-h-0 flex-1">
        <ReadOnlyCode
          text={content.text}
          label="Cell value"
          language={content.language}
          wrap={wrap}
        />
      </div>
    </div>
  );
}

type Content =
  | { kind: "code"; text: string; language: CodeLanguage; note: string | null }
  | { kind: "binary"; base64: string; size: number };

function describe(value: Value, category: GridColumn["category"]): Content {
  if (value.t === "bytes") return { kind: "binary", base64: value.v, size: value.len };
  if (value.t === "null")
    return { kind: "code", text: "NULL", language: "text", note: "The value is NULL." };
  const text = rawText(value);
  if (value.t === "json" || category === "json") {
    const pretty = value.t === "json" && value.truncated ? null : prettyJson(text);
    return pretty === null
      ? {
          kind: "code",
          text,
          language: "text",
          note: "Not valid JSON (or cut short); shown as text.",
        }
      : { kind: "code", text: pretty, language: "json", note: null };
  }
  if (value.t === "xml" || category === "xml") {
    const pretty = value.t === "xml" && value.truncated ? null : prettyXml(text);
    return pretty === null
      ? {
          kind: "code",
          text,
          language: "text",
          note: "Not well-formed XML (or cut short); shown as text.",
        }
      : { kind: "code", text: pretty, language: "xml", note: null };
  }
  return detect(text, value.t === "text" && value.truncated === true);
}

/** The editor's language: the column's, or what the stored text holds. Taken from the value, not
 * the draft, so typing never switches it (which would rebuild the editor). */
function editLanguage(value: Value, category: GridColumn["category"]): CodeLanguage {
  if (value.t === "json" || category === "json") return "json";
  if (value.t === "xml" || category === "xml") return "xml";
  if (value.t !== "text") return "text";
  const content = detect(value.v, false);
  return content.kind === "code" ? content.language : "text";
}

/** Text that holds JSON or XML is shown as such: JSON is tried first, then XML, then plain text.
 * A value cut short cannot parse, so it stays text. */
function detect(text: string, truncated: boolean): Content {
  const plain: Content = { kind: "code", text, language: "text", note: null };
  const start = text.trimStart()[0];
  if (truncated || start === undefined) return plain;
  if (start === "{" || start === "[") {
    const json = prettyJson(text);
    if (json !== null)
      return {
        kind: "code",
        text: json,
        language: "json",
        note: "Text that holds JSON, shown as JSON.",
      };
  }
  if (start === "<") {
    const xml = prettyXml(text);
    if (xml !== null)
      return {
        kind: "code",
        text: xml,
        language: "xml",
        note: "Text that holds XML, shown as XML.",
      };
  }
  return plain;
}

function Binary({ base64, size }: { base64: string; size: number }) {
  const bytes = useMemo(() => decodeBase64(base64), [base64]);
  const type = imageType(bytes);
  const url = useMemo(
    () => (type ? URL.createObjectURL(new Blob([bytes.slice().buffer], { type })) : null),
    [bytes, type],
  );
  useEffect(
    () => () => {
      if (url) URL.revokeObjectURL(url);
    },
    [url],
  );
  return (
    <div className="flex h-full flex-col overflow-auto">
      <p className="px-2 py-1 text-[11px] text-zinc-500">
        {formatSize(size)}
        {bytes.length < size && ` (first ${formatSize(bytes.length)} shown)`}
        {type && ` · ${type}`}
      </p>
      {url && (
        <img
          src={url}
          alt="Image preview of the cell"
          className="max-h-64 self-start object-contain px-2"
        />
      )}
      <pre aria-label="Hex dump" className="px-2 py-1 font-mono text-[11px] text-zinc-300">
        {hexDump(bytes)}
      </pre>
    </div>
  );
}
