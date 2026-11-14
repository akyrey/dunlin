/** One thin typed wrapper per backend command (docs/IPC.md). */
import type {
  ConnectionDraft,
  ConnectionProfile,
  ConnectionSecrets,
  ConnectionTestResult,
  DriverInfo,
  KnownHost,
  SshKnownHostRemoveArgs,
  SshTrustHostArgs,
  QueryExecuteArgs,
  QueryResult,
  SessionInfo,
  StatementRange,
} from "./generated";
import { call } from "./invoke";

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
