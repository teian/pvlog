const CALENDAR_RESOLUTIONS = new Set(["daily", "monthly", "yearly"]);

/** Formats an epoch-millisecond timestamp for chart axes/tooltips in the query's timezone and the active locale. @param epochMillis - Timestamp in Unix epoch milliseconds. @param resolution - Resolution actually returned by the series query. @param timezone - IANA timezone used for calendar bucket boundaries. @param locale - Active i18next locale. @returns A localized, timezone-aware label. */
export function formatTimestamp(
  epochMillis: number,
  resolution: string,
  timezone: string,
  locale: string,
): string {
  if (!Number.isFinite(epochMillis)) return "";
  const options: Intl.DateTimeFormatOptions = CALENDAR_RESOLUTIONS.has(
    resolution,
  )
    ? { dateStyle: "medium", timeZone: timezone }
    : { dateStyle: "short", timeStyle: "short", timeZone: timezone };
  return new Intl.DateTimeFormat(locale, options).format(epochMillis);
}
