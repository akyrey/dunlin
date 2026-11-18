import { Button, Modal } from "@roost/ui";

type Props = {
  count: number;
  /** What the user was about to do, e.g. "change the sort". */
  action: string;
  onApply: () => void;
  onDiscard: () => void;
  onCancel: () => void;
};

/** Asked before anything that would lose pending edits (task 7.13). */
export function UnsavedDialog({ count, action, onApply, onDiscard, onCancel }: Props) {
  return (
    <Modal
      title="Unapplied changes"
      onClose={onCancel}
      footer={
        <>
          <Button onClick={onCancel} autoFocus>
            Cancel
          </Button>
          <Button variant="danger" onClick={onDiscard}>
            Discard
          </Button>
          <Button variant="primary" onClick={onApply}>
            Apply
          </Button>
        </>
      }
    >
      <p className="text-sm text-zinc-200">
        {count} {count === 1 ? "change has" : "changes have"} not been applied. Apply or discard{" "}
        {count === 1 ? "it" : "them"} to {action}.
      </p>
    </Modal>
  );
}
