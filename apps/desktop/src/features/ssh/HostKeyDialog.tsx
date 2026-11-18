import { Button, Modal } from "@roost/ui";
import type { SshTrustHostArgs } from "../../ipc";

type Props = {
  hostKey: SshTrustHostArgs;
  /** A changed key can only be refused (SECURITY.md#ssh). */
  changed: boolean;
  onTrust: () => void;
  onCancel: () => void;
};

/** Shows an SSH host key fingerprint and asks whether to trust it, or refuses a changed key. */
export function HostKeyDialog({ hostKey, changed, onTrust, onCancel }: Props) {
  const where = hostKey.port === 22 ? hostKey.host : `${hostKey.host}:${hostKey.port}`;
  return (
    <Modal
      title={changed ? "SSH host key has changed" : "Trust this SSH server?"}
      onClose={onCancel}
      footer={
        changed ? (
          <Button onClick={onCancel} autoFocus>
            Close
          </Button>
        ) : (
          <>
            <Button onClick={onCancel} autoFocus>
              Cancel
            </Button>
            <Button variant="primary" onClick={onTrust}>
              Trust and connect
            </Button>
          </>
        )
      }
    >
      <div className="flex flex-col gap-3 text-sm">
        {changed ? (
          <p role="alert" className="text-rose-300">
            The key presented by {where} is not the one trusted before. Someone may be intercepting
            the connection, or the server was reinstalled. Dunlin will not connect. If you are sure
            the change is legitimate, remove the old key under Settings › Trusted SSH hosts (or from
            your ~/.ssh/known_hosts) and connect again.
          </p>
        ) : (
          <p className="text-zinc-300">
            {where} is not known yet. Compare this fingerprint with the one your administrator gave
            you before trusting it.
          </p>
        )}
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt className="text-zinc-500">Key type</dt>
          <dd className="font-mono">{hostKey.algorithm}</dd>
          <dt className="text-zinc-500">Fingerprint</dt>
          <dd className="break-all font-mono" data-testid="fingerprint">
            {hostKey.fingerprint}
          </dd>
        </dl>
      </div>
    </Modal>
  );
}
