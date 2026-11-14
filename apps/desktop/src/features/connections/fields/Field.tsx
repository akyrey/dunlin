import type { ReactNode } from "react";

interface FieldProps {
  label: string;
  children: ReactNode;
  /**
   * The control's id. Required for selects: wrapping a select in its label would make every
   * option text part of its accessible name.
   */
  htmlFor?: string;
}

export function Field({ label, children, htmlFor }: FieldProps) {
  if (htmlFor !== undefined) {
    return (
      <div className="flex flex-col gap-1">
        <label htmlFor={htmlFor} className="text-xs text-zinc-400">
          {label}
        </label>
        {children}
      </div>
    );
  }
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs text-zinc-400">{label}</span>
      {children}
    </label>
  );
}
