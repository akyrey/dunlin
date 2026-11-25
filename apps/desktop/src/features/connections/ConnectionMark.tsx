import type { DriverKind } from "../../ipc";
import { DriverIcon } from "./DriverIcon";

type Props = { driver: DriverKind; color: string | null };

/**
 * The connection's colour dot and its database's logo, so every place that names a
 * connection (the list, the schema tree, the key browser) looks the same.
 */
export function ConnectionMark({ driver, color }: Props) {
  return (
    <>
      <span
        aria-hidden="true"
        data-connection-color
        className="h-2.5 w-2.5 shrink-0 rounded-full"
        style={{ backgroundColor: color ?? "transparent" }}
      />
      <DriverIcon driver={driver} className="h-3.5 w-3.5 text-zinc-400" />
    </>
  );
}
