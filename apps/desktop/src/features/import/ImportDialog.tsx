import { useEffect, useMemo, useRef, useState } from "react";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { toast } from "../../components/toastStore";
import {
  type AppError,
  type CsvOptions,
  type CsvPreview,
  type TableInfo,
  importCsv,
  importPreview,
  pickCsvFile,
  queryCancel,
  tabClose,
  toAppError,
} from "../../ipc";
import { autoMap, delimiterFor, importColumns, type Mapping } from "./mapping";

type Props = {
  sessionId: string;
  info: TableInfo;
  /** The import committed: the table has new rows. */
  onImported: () => void;
  onClose: () => void;
};

const DELIMITERS: { value: string; label: string }[] = [
  { value: ",", label: "Comma" },
  { value: ";", label: "Semicolon" },
  { value: "\t", label: "Tab" },
  { value: "|", label: "Bar" },
];
/** Preview rows shown under the mapping. */
const SAMPLE_ROWS = 5;

type Running = { importId: string; rows: number };

/** Escape does not close the dialog while the import runs; Cancel import stops it. */
const keepOpen = () => undefined;

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Imports a CSV file into a table: options, column mapping, a preview, then one transaction. */
export function ImportDialog({ sessionId, info, onImported, onClose }: Props) {
  const [path, setPath] = useState<string | null>(null);
  const [options, setOptions] = useState<CsvOptions>({ delimiter: ",", header: true, null: "" });
  const [preview, setPreview] = useState<CsvPreview | null>(null);
  const [mapping, setMapping] = useState<Mapping>({});
  const [error, setError] = useState<AppError | null>(null);
  const [running, setRunning] = useState<Running | null>(null);
  const writable = useMemo(() => info.columns.filter((c) => !c.generated), [info]);
  /** The headers the mapping was guessed from; other option changes keep the user's mapping. */
  const mappedFrom = useRef("");

  // The file is chosen first; closing the picker closes the dialog.
  useEffect(() => {
    let stale = false;
    void pickCsvFile().then((picked) => {
      if (stale) return;
      if (picked === null) {
        onClose();
        return;
      }
      setOptions((current) => ({ ...current, delimiter: delimiterFor(picked) }));
      setPath(picked);
    });
    return () => {
      stale = true;
    };
  }, [onClose]);

  useEffect(() => {
    if (path === null) return;
    let stale = false;
    importPreview({ path, options }).then(
      (read) => {
        if (stale) return;
        setPreview(read);
        const key = JSON.stringify([read.headers, options.header]);
        if (key !== mappedFrom.current) {
          mappedFrom.current = key;
          setMapping(autoMap(read.headers, info.columns, options.header));
        }
        setError(null);
      },
      (caught: unknown) => {
        if (stale) return;
        setPreview(null);
        setError(toAppError(caught));
      },
    );
    return () => {
      stale = true;
    };
  }, [path, options, info]);

  const columns = importColumns(mapping);
  const start = async () => {
    if (path === null) return;
    const importId = crypto.randomUUID();
    setRunning({ importId, rows: 0 });
    setError(null);
    try {
      const done = await importCsv(
        { sessionId, importId, table: info.table, path, options, columns },
        (rows) => {
          setRunning({ importId, rows });
        },
      );
      const rows = Number(done.rowsInserted).toLocaleString("en-US");
      toast.success(`Imported ${rows} rows into ${info.table.name}`);
      onImported();
    } catch (caught: unknown) {
      setError(toAppError(caught));
      setRunning(null);
    } finally {
      tabClose(sessionId, importId).catch(() => {
        // The session may already be closed, which closed this connection too.
      });
    }
  };

  const footer = running ? (
    <>
      <span role="status" className="mr-auto self-center text-xs text-zinc-400">
        Imported {running.rows.toLocaleString("en-US")} rows…
      </span>
      <Button variant="danger" onClick={() => void queryCancel(sessionId, running.importId)}>
        Cancel import
      </Button>
    </>
  ) : (
    <>
      <Button variant="ghost" onClick={onClose}>
        Cancel
      </Button>
      <Button
        variant="primary"
        disabled={!preview || columns.length === 0}
        onClick={() => void start()}
      >
        Import
      </Button>
    </>
  );

  return (
    <Modal
      title={`Import CSV into ${info.table.name}`}
      onClose={running ? keepOpen : onClose}
      footer={footer}
    >
      {path === null ? (
        <p className="text-sm text-zinc-400">Choose a CSV file…</p>
      ) : (
        <div className="flex max-h-[60vh] flex-col gap-3 overflow-auto text-sm">
          <p className="truncate font-mono text-xs text-zinc-400" title={path}>
            {fileName(path)}
          </p>
          <fieldset disabled={running !== null} className="flex flex-wrap items-end gap-3 text-xs">
            <label className="flex flex-col gap-1">
              Delimiter
              <select
                className="input py-0.5"
                value={options.delimiter}
                onChange={(e) => {
                  setOptions({ ...options, delimiter: e.target.value });
                }}
              >
                {DELIMITERS.map((d) => (
                  <option key={d.label} value={d.value}>
                    {d.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={options.header}
                onChange={(e) => {
                  setOptions({ ...options, header: e.target.checked });
                }}
              />
              First row is a header
            </label>
            <label className="flex flex-col gap-1">
              NULL text
              <input
                className="input w-24 py-0.5 font-mono"
                placeholder="(empty)"
                value={options.null}
                onChange={(e) => {
                  setOptions({ ...options, null: e.target.value });
                }}
              />
            </label>
          </fieldset>
          {preview && (
            <MappingTable
              columns={writable.map((c) => ({ name: c.name, type: c.typeName }))}
              preview={preview}
              mapping={mapping}
              disabled={running !== null}
              onChange={setMapping}
            />
          )}
          {error && (
            <p role="alert" className="text-rose-300">
              {error.message}
            </p>
          )}
          <p className="text-xs text-zinc-500">
            Every row goes in one transaction: if one is refused, nothing is imported. Unquoted
            fields equal to the NULL text are NULL; quoted fields are always text.
          </p>
        </div>
      )}
    </Modal>
  );
}

type MappingProps = {
  columns: { name: string; type: string }[];
  preview: CsvPreview;
  mapping: Mapping;
  disabled: boolean;
  onChange: (mapping: Mapping) => void;
};

function sample(preview: CsvPreview, index: number | null | undefined): string {
  if (index === null || index === undefined) return "default";
  return preview.rows
    .slice(0, SAMPLE_ROWS)
    .map((row) => row[index] ?? "NULL")
    .join(", ");
}

/** One row per table column: the CSV field that fills it, and that field's first values. */
function MappingTable({ columns, preview, mapping, disabled, onChange }: MappingProps) {
  return (
    <table aria-label="Column mapping" className="w-full text-xs">
      <thead className="text-left text-zinc-500">
        <tr>
          <th className="py-1 font-normal">Column</th>
          <th className="py-1 font-normal">From CSV</th>
          <th className="py-1 font-normal">First values</th>
        </tr>
      </thead>
      <tbody>
        {columns.map(({ name, type }) => (
          <tr key={name} className="border-t border-zinc-800">
            <td className="py-1 pr-2 font-mono" title={type}>
              {name}
            </td>
            <td className="py-1 pr-2">
              <select
                aria-label={`Source for ${name}`}
                className="input py-0.5"
                disabled={disabled}
                value={mapping[name] ?? ""}
                onChange={(e) => {
                  const value = e.target.value;
                  onChange({ ...mapping, [name]: value === "" ? null : Number(value) });
                }}
              >
                <option value="">— default —</option>
                {preview.headers.map((header, index) => (
                  <option key={`${index}:${header}`} value={index}>
                    {header}
                  </option>
                ))}
              </select>
            </td>
            <td className="max-w-40 truncate py-1 font-mono text-zinc-400">
              {sample(preview, mapping[name])}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
