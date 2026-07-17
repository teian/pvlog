import type { ReactNode } from "react";

/** Labels one numbered wizard section. @param props - Section number and localized title. @returns Compact handoff-style heading. */
export function SystemWizardSectionHeading({
  number,
  children,
}: {
  number: number;
  children: ReactNode;
}) {
  return (
    <h2 className="flex items-center gap-2 text-[13px] font-extrabold tracking-[0.02em]">
      <span className="flex size-5 items-center justify-center rounded-full bg-primary font-mono text-[11px] text-primary-foreground">
        {number}
      </span>
      {children}
    </h2>
  );
}
