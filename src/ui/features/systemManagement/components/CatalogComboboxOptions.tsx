import { ComboboxItem, ComboboxList } from "@/shared/components";
import { cn } from "@/shared/lib/utils";

export interface CatalogComboboxOption<T> {
  id: string;
  label: string;
  description: string;
  entry: T | null;
  manual?: boolean;
}

/** Renders the catalog result list and its optional loading or empty status. */
export function CatalogComboboxOptions({
  statusText,
}: {
  statusText?: string | undefined;
}) {
  return (
    <>
      <ComboboxList>
        {(option: CatalogComboboxOption<unknown>) => (
          <ComboboxItem key={option.id} value={option}>
            <span className="flex min-w-0 flex-col gap-0.5">
              <span
                className={cn(
                  "truncate font-medium",
                  option.manual && "text-primary",
                )}
              >
                {option.label}
              </span>
              <span className="truncate text-xs text-muted-foreground">
                {option.description}
              </span>
            </span>
          </ComboboxItem>
        )}
      </ComboboxList>
      {statusText ? (
        <p
          className="border-t border-border px-3 py-2 text-xs text-muted-foreground"
          role="status"
        >
          {statusText}
        </p>
      ) : null}
    </>
  );
}
