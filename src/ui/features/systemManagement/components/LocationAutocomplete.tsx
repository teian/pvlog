/* eslint-disable complexity, max-lines-per-function -- the async combobox keeps input, keyboard, loading, result, and attribution states in one ARIA-owned unit */
import {
  searchAddresses,
  type GeocodingResult,
} from "@/features/systemManagement/api/systemManagementApi";
import { Button, Field, FieldLabel, Input } from "@/shared/components";
import { LoaderCircleIcon, MapPinIcon, SearchIcon } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";

const MIN_QUERY_LENGTH = 3;
const SEARCH_DELAY_MS = 350;

/** Resolves typed addresses into keyboard-selectable OpenStreetMap suggestions. @param props - Current address and change callback. @returns Accessible asynchronous combobox. */
export function LocationAutocomplete({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const listId = useId();
  const [results, setResults] = useState<GeocodingResult[]>([]);
  const [selected, setSelected] = useState<GeocodingResult>();
  const [activeIndex, setActiveIndex] = useState(0);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState(false);
  const [searched, setSearched] = useState(false);
  const open = results.length > 0 && selected === undefined;

  useEffect(() => {
    const query = value.trim();
    if (query.length < MIN_QUERY_LENGTH || selected) {
      return;
    }
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      setPending(true);
      setError(false);
      void searchAddresses(query, i18n.language, 5, controller.signal)
        .then((items) => {
          setResults(items);
          setActiveIndex(0);
          setSearched(true);
        })
        .catch((reason: unknown) => {
          if (!(
            reason instanceof DOMException && reason.name === "AbortError"
          )) {
            setError(true);
          }
        })
        .finally(() => {
          if (!controller.signal.aborted) setPending(false);
        });
    }, SEARCH_DELAY_MS);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [i18n.language, selected, value]);

  const choose = (result: GeocodingResult) => {
    setSelected(result);
    setResults([]);
    setError(false);
    onChange(result.displayName);
  };

  return (
    <Field>
      <FieldLabel htmlFor="system-location">
        {t("systemManagement.wizard.location")}
      </FieldLabel>
      <div className="relative">
        <div className="flex gap-2">
          <Input
            aria-activedescendant={
              open ? `${listId}-${String(activeIndex)}` : undefined
            }
            aria-autocomplete="list"
            aria-controls={listId}
            aria-expanded={open}
            autoComplete="off"
            id="system-location"
            onChange={(event) => {
              if (event.target.value.trim().length < MIN_QUERY_LENGTH) {
                setResults([]);
              }
              setSelected(undefined);
              setSearched(false);
              onChange(event.target.value);
            }}
            onKeyDown={(event) => {
              if (!open) return;
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                const direction = event.key === "ArrowDown" ? 1 : -1;
                setActiveIndex(
                  (current) =>
                    (current + direction + results.length) % results.length,
                );
              } else if (event.key === "Enter") {
                event.preventDefault();
                const result = results[activeIndex];
                if (result) choose(result);
              } else if (event.key === "Escape") {
                setResults([]);
              }
            }}
            role="combobox"
            value={value}
          />
          <Button
            disabled={results.length === 0 || pending}
            onClick={() => {
              const result = results[activeIndex];
              if (result) choose(result);
            }}
            type="button"
          >
            {pending ? (
              <LoaderCircleIcon
                className="animate-spin motion-reduce:animate-none"
                data-icon="inline-start"
              />
            ) : (
              <SearchIcon data-icon="inline-start" />
            )}
            {t("systemManagement.actions.search")}
          </Button>
        </div>
        {open ? (
          <div
            className="absolute inset-x-0 top-full z-10 mt-1 max-h-64 overflow-y-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
            id={listId}
            role="listbox"
          >
            {results.map((result, index) => (
              <button
                aria-selected={index === activeIndex}
                className="flex w-full items-start gap-2 rounded-sm px-3 py-2 text-left text-sm hover:bg-accent focus-visible:bg-accent focus-visible:outline-none aria-selected:bg-accent"
                id={`${listId}-${String(index)}`}
                key={`${String(result.latitude)}-${String(result.longitude)}`}
                onClick={() => {
                  choose(result);
                }}
                onMouseEnter={() => {
                  setActiveIndex(index);
                }}
                role="option"
                type="button"
              >
                <MapPinIcon
                  aria-hidden="true"
                  className="mt-0.5 shrink-0 text-primary"
                />
                <span className="min-w-0 flex-1">
                  <span className="block">{result.displayName}</span>
                  <span className="font-mono text-[10px] text-muted-foreground">
                    {result.latitude.toFixed(6)}
                    {", "}
                    {result.longitude.toFixed(6)}
                  </span>
                </span>
              </button>
            ))}
          </div>
        ) : null}
      </div>
      {selected ? (
        <div className="flex flex-col items-start gap-3 rounded-md border bg-muted/30 p-4 text-sm sm:flex-row sm:items-center">
          <MapPinIcon aria-hidden="true" className="text-primary" />
          <span className="min-w-0 flex-1">
            <span className="block">{selected.displayName}</span>
            <span className="text-[10px] text-muted-foreground">
              {selected.attribution}
            </span>
          </span>
          <output className="font-mono text-xs text-muted-foreground">
            {selected.latitude.toFixed(6)}
            {", "}
            {selected.longitude.toFixed(6)}
          </output>
        </div>
      ) : null}
      {searched && results.length === 0 && !selected && !pending ? (
        <p className="text-xs text-warning">
          {t("systemManagement.wizard.locationNotFound")}
        </p>
      ) : null}
      {error ? (
        <p className="text-xs text-destructive">
          {t("systemManagement.wizard.locationError")}
        </p>
      ) : null}
    </Field>
  );
}
