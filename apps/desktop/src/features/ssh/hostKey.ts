import type { AppError, SshTrustHostArgs } from "../../ipc";

const STAGE_LABELS: Record<string, string> = {
  ssh_connect: "Could not reach the SSH server",
  host_key: "SSH host key check failed",
  ssh_auth: "SSH authentication failed",
  forward: "The SSH server could not reach the database",
  db_connect: "Could not connect to the database",
  db_auth: "Database authentication failed",
};

function detailOf(error: AppError): Record<string, unknown> | null {
  const detail = error.detail;
  return typeof detail === "object" && detail !== null ? (detail as Record<string, unknown>) : null;
}

/** The key to trust from an `ssh_host_key_unknown` or `ssh_host_key_changed` error. */
export function hostKeyFrom(error: AppError): SshTrustHostArgs | null {
  const detail = detailOf(error);
  if (!detail) return null;
  const { host, port, algorithm, fingerprint } = detail;
  if (
    typeof host !== "string" ||
    typeof port !== "number" ||
    typeof algorithm !== "string" ||
    typeof fingerprint !== "string"
  ) {
    return null;
  }
  return { host, port, algorithm, fingerprint };
}

/** "SSH authentication failed" etc., when the backend reported which stage failed. */
export function stageLabel(error: AppError): string | null {
  const stage = detailOf(error)?.stage;
  return typeof stage === "string" ? (STAGE_LABELS[stage] ?? null) : null;
}

/** The message to show for a failed connection attempt, prefixed with its stage. */
export function describeConnectError(error: AppError): string {
  const label = stageLabel(error);
  return label ? `${label}: ${error.message}` : error.message;
}
