import { Label } from "@/shared/components";

/** Snapshot editor properties. */
export interface SnapshotEditorProps {
  /** Identifier tying the label to the textarea. */ id: string;
  /** Localized field label. */ label: string;
  /** Called with the edited snapshot text. */ onChange: (
    value: string,
  ) => void;
  /** Current snapshot text. */ value: string;
}

/** Renders a labelled monospace editor for a catalog snapshot document. @param props - Field identity, label, value and change callback. @returns The snapshot editor. */
export function SnapshotEditor({
  id,
  label,
  onChange,
  value,
}: SnapshotEditorProps) {
  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      <textarea
        className="min-h-40 w-full rounded-md border border-input bg-background p-3 font-mono text-xs focus-visible:ring-2 focus-visible:ring-ring"
        id={id}
        onChange={(event) => {
          onChange(event.target.value);
        }}
        value={value}
      />
    </div>
  );
}
