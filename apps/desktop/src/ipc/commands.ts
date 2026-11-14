/** One thin typed wrapper per backend command (docs/IPC.md). */
import type {
  Settings,
  SavedQuery,
  SavedQueryDraft,
  ExportDone,
  ExportProgress,
  ExportResultArgs,
  ExportTableArgs,
  ExportTextArgs,
  ApplyResult,
  ChangesArgs,
  ConnectionDraft,
  ConnectionProfile,
  ConnectionSecrets,
  CellFullValueArgs,
  CompletionSchema,
  ConnectionTestResult,
  DriverInfo,
  HistoryEntry,
  HistoryListArgs,
  KnownHost,
  Namespace,
  NamespaceRef,
  ObjectSummary,
  SshKnownHostRemoveArgs,
  SshTrustHostArgs,
  QueryExecuteArgs,
  RowCount,
  QueryResult,
  SessionInfo,
  StatementRange,
  TableCountArgs,
  TableInfo,
  TablePage,
  TablePageArgs,
  TableRef,
  TableSnippets,
  Value,
} from "./generated";
import { Channel } from "@tauri-apps/api/core";

import { call, callWith } from "./invoke";

export const driverList = (): Promise<DriverInfo[]> => call("driver_list");

export const connectionList = (): Promise<ConnectionProfile[]> => call("connection_list");

export const connectionSave = (
  profile: ConnectionDraft,
  secrets?: ConnectionSecrets,
): Promise<ConnectionProfile> => call("connection_save", { profile, secrets });

export const connectionDelete = (connectionId: string): Promise<void> =>
  call("connection_delete", { connectionId });

export const connectionTest = (
  profile: ConnectionDraft,
  secrets?: ConnectionSecrets,
): Promise<ConnectionTestResult> => call("connection_test", { profile, secrets });

export const sessionOpen = (connectionId: string): Promise<SessionInfo> =>
  call("session_open", { connectionId });

export const sessionClose = (sessionId: string): Promise<void> =>
  call("session_close", { sessionId });

export const sqlSplit = (sessionId: string, text: string): Promise<StatementRange[]> =>
  call("sql_split", { sessionId, text });

export const queryExecute = (args: QueryExecuteArgs): Promise<QueryResult> =>
  call("query_execute", args);

export const queryCancel = (sessionId: string, tabId: string): Promise<void> =>
  call("query_cancel", { sessionId, tabId });

export const sshTrustHost = (args: SshTrustHostArgs): Promise<void> => call("ssh_trust_host", args);

export const sshKnownHostsList = (): Promise<KnownHost[]> => call("ssh_known_hosts_list");

export const sshKnownHostRemove = (args: SshKnownHostRemoveArgs): Promise<void> =>
  call("ssh_known_host_remove", args);

export const tabClose = (sessionId: string, tabId: string): Promise<void> =>
  call("tab_close", { sessionId, tabId });

export const schemaNamespaces = (sessionId: string): Promise<Namespace[]> =>
  call("schema_namespaces", { sessionId });

export const schemaObjects = (
  sessionId: string,
  namespace: NamespaceRef,
): Promise<ObjectSummary[]> => call("schema_objects", { sessionId, namespace });

export const schemaCompletion = (
  sessionId: string,
  namespace: NamespaceRef,
): Promise<CompletionSchema> => call("schema_completion", { sessionId, namespace });

export const schemaTable = (sessionId: string, table: TableRef): Promise<TableInfo> =>
  call("schema_table", { sessionId, table });

export const schemaTableDdl = (sessionId: string, table: TableRef): Promise<string> =>
  call("schema_table_ddl", { sessionId, table });

export const schemaSnippets = (sessionId: string, table: TableRef): Promise<TableSnippets> =>
  call("schema_snippets", { sessionId, table });

/** Clears the session's metadata cache; callers then reload what they show. */
export const schemaRefresh = (sessionId: string): Promise<void> =>
  call("schema_refresh", { sessionId });

export const historyList = (args: HistoryListArgs): Promise<HistoryEntry[]> =>
  call("history_list", args);

export const historyClear = (connectionId: string): Promise<void> =>
  call("history_clear", { connectionId });

export const tablePage = (args: TablePageArgs): Promise<TablePage> => call("table_page", args);

export const tableCount = (args: TableCountArgs): Promise<RowCount> => call("table_count", args);

export const cellFullValue = (args: CellFullValueArgs): Promise<Value> =>
  call("cell_full_value", args);

export const changesPreview = (args: ChangesArgs): Promise<string[]> =>
  call("changes_preview", args);

export const changesApply = (args: ChangesArgs): Promise<ApplyResult> =>
  call("changes_apply", args);

export const exportText = (args: ExportTextArgs): Promise<string> => call("export_text", args);

export const exportResult = (args: ExportResultArgs): Promise<ExportDone> =>
  call("export_result", args);

/** Streams a whole table to a file; `onProgress` receives the rows written so far. */
export function exportTable(
  args: ExportTableArgs,
  onProgress: (rows: number) => void,
): Promise<ExportDone> {
  const channel = new Channel<ExportProgress>();
  channel.onmessage = (progress) => {
    onProgress(Number(progress.rowsWritten));
  };
  return callWith("export_table", args, { onProgress: channel });
}

export const savedQueryList = (): Promise<SavedQuery[]> => call("saved_query_list");

export const savedQuerySave = (query: SavedQueryDraft): Promise<SavedQuery> =>
  call("saved_query_save", { query });

export const savedQueryDelete = (id: string): Promise<void> => call("saved_query_delete", { id });

export const settingsGet = (): Promise<Settings> => call("settings_get");

export const settingsSet = (settings: Settings): Promise<Settings> =>
  call("settings_set", { settings });

/** The folder the backend writes its log files to. */
export const logsDir = (): Promise<string> => call("logs_dir");
