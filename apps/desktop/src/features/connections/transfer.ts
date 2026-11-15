import { toast } from "../../components/toastStore";
import {
  connectionsExport,
  connectionsImport,
  pickConnectionsExportFile,
  pickConnectionsFile,
  toAppError,
} from "../../ipc";
import { useConnections } from "./connectionsStore";

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** Asks for a file and writes every connection's settings to it; passwords are not included. */
export async function exportConnections(): Promise<void> {
  try {
    const path = await pickConnectionsExportFile();
    if (!path) return;
    const count = await connectionsExport(path);
    toast.success(`Exported ${plural(count, "connection")}. Passwords were not included.`);
  } catch (caught) {
    toast.error(toAppError(caught).message);
  }
}

/** Asks for a connections file and adds its connections; says which were skipped and why. */
export async function importConnections(): Promise<void> {
  try {
    const path = await pickConnectionsFile();
    if (!path) return;
    const report = await connectionsImport(path);
    await useConnections.getState().load();
    toast.success(
      `Imported ${plural(report.imported.length, "connection")}. Enter their passwords when you first connect.`,
    );
    if (report.skipped.length > 0)
      toast.error(
        `Skipped ${plural(report.skipped.length, "connection")}: ${report.skipped.join("; ")}`,
      );
  } catch (caught) {
    toast.error(toAppError(caught).message);
  }
}
