import type { SshConfigEntry, SshHostSettings, SshSettings } from "../../ipc";
import { DEFAULT_SSH_PORT } from "./draft";

function hostOf(entry: SshConfigEntry): SshHostSettings {
  return {
    host: entry.hostName ?? entry.alias,
    port: entry.port ?? DEFAULT_SSH_PORT,
    user: entry.user ?? "",
    auth: entry.identityFile ? "key_file" : "agent",
    keyFile: entry.identityFile,
  };
}

/** `[user@]host[:port]`, the form `ProxyJump` takes besides an alias. */
function parseHop(hop: string): SshHostSettings {
  const at = hop.lastIndexOf("@");
  const user = at >= 0 ? hop.slice(0, at) : "";
  const rest = at >= 0 ? hop.slice(at + 1) : hop;
  const match = /^(.*):(\d+)$/.exec(rest);
  const port = match ? Number.parseInt(match[2] ?? "", 10) : DEFAULT_SSH_PORT;
  return {
    host: match ? (match[1] ?? rest) : rest,
    port: Number.isInteger(port) && port > 0 && port < 65536 ? port : DEFAULT_SSH_PORT,
    user,
    auth: "agent",
    keyFile: null,
  };
}

/** The SSH settings for `alias` as `~/.ssh/config` describes it; its `ProxyJump` becomes the
 * jump host, read from the same file when it names another alias. */
export function settingsFromConfig(
  alias: string,
  entries: readonly SshConfigEntry[],
): SshSettings | null {
  const entry = entries.find((e) => e.alias === alias);
  if (!entry) return null;
  const hop = entry.proxyJump;
  const jumpEntry = hop ? entries.find((e) => e.alias === hop) : undefined;
  return {
    server: hostOf(entry),
    jump: hop ? (jumpEntry ? hostOf(jumpEntry) : parseHop(hop)) : null,
  };
}
