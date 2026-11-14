import { useEffect, useState } from "react";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import {
  type AppError,
  type KnownHost,
  sshKnownHostRemove,
  sshKnownHostsList,
  toAppError,
} from "../../ipc";

/** Settings: SSH host keys trusted in Dunlin. ~/.ssh/known_hosts is read but never changed. */
export function TrustedHosts({ onClose }: { onClose: () => void }) {
  const [hosts, setHosts] = useState<KnownHost[] | null>(null);
  const [error, setError] = useState<AppError | null>(null);

  const reload = async () => {
    try {
      setHosts(await sshKnownHostsList());
    } catch (caught) {
      setError(toAppError(caught));
    }
  };

  useEffect(() => {
    let closed = false;
    sshKnownHostsList().then(
      (loaded) => {
        if (!closed) setHosts(loaded);
      },
      (caught: unknown) => {
        if (!closed) setError(toAppError(caught));
      },
    );
    return () => {
      closed = true;
    };
  }, []);

  const remove = async (host: KnownHost) => {
    try {
      await sshKnownHostRemove({ host: host.host, port: host.port, algorithm: host.algorithm });
      await reload();
    } catch (caught) {
      setError(toAppError(caught));
    }
  };

  return (
    <Modal
      title="Trusted SSH hosts"
      onClose={onClose}
      footer={<Button onClick={onClose}>Close</Button>}
    >
      <div className="flex flex-col gap-2 text-sm">
        <p className="text-xs text-zinc-400">
          Keys you trusted in Dunlin. Keys in ~/.ssh/known_hosts are also trusted and are managed
          with your usual tools.
        </p>
        {error && (
          <p role="alert" className="text-xs text-rose-300">
            {error.message}
          </p>
        )}
        {hosts?.length === 0 && <p className="text-zinc-500">No hosts trusted yet.</p>}
        <ul aria-label="Trusted hosts" className="flex flex-col gap-1">
          {hosts?.map((host) => (
            <li
              key={`${host.host}:${host.port}:${host.algorithm}`}
              className="flex items-center gap-2 rounded bg-zinc-950 px-2 py-1"
            >
              <span className="flex-1 truncate">
                <span className="block">
                  {host.host}:{host.port}
                </span>
                <span className="block font-mono text-[10px] text-zinc-500">
                  {host.algorithm} {host.fingerprint}
                </span>
              </span>
              <Button
                variant="ghost"
                aria-label={`Remove ${host.host}:${host.port}`}
                onClick={() => void remove(host)}
              >
                Remove
              </Button>
            </li>
          ))}
        </ul>
      </div>
    </Modal>
  );
}
