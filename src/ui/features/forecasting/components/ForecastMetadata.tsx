/** Forecast metadata item properties. */
export interface ForecastMetadataProps {
  /** Localized label. */ label: string;
  /** Display value. */ value: string;
}

/** Renders one labeled forecast metadata value. @param props - Label and display value. @returns Definition-list item. */
export function ForecastMetadata({ label, value }: ForecastMetadataProps) {
  return (
    <div>
      <dt className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
        {label}
      </dt>
      <dd className="mt-1 break-words font-mono text-sm tabular-nums">
        {value}
      </dd>
    </div>
  );
}
