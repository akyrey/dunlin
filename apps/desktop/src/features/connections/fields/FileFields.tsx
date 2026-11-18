import { Button } from "@roost/ui";
import { pickDatabaseFile, pickNewDatabaseFile } from "../../../ipc";
import { Field } from "./Field";

const CREATE_OPTION = "create_if_missing";

type Props = {
  filePath: string | null;
  options: Record<string, string>;
  onChange: (patch: { filePath?: string; options?: Record<string, string> }) => void;
};

/** A database file: typed, picked, or created (which sets the driver's create option). */
export function FileFields({ filePath, options, onChange }: Props) {
  const choose = async (create: boolean) => {
    const path = create ? await pickNewDatabaseFile() : await pickDatabaseFile();
    if (path)
      onChange({ filePath: path, options: { ...options, [CREATE_OPTION]: String(create) } });
  };
  return (
    <Field label="Database file">
      <div className="flex gap-2">
        <input
          className="input flex-1 font-mono text-xs"
          value={filePath ?? ""}
          placeholder="/path/to/database.db"
          onChange={(e) => {
            onChange({ filePath: e.target.value });
          }}
        />
        <Button onClick={() => void choose(false)}>Browse…</Button>
        <Button onClick={() => void choose(true)}>New file…</Button>
      </div>
    </Field>
  );
}
