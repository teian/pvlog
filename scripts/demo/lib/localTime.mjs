/** Local calendar values of an instant in the demo systems' timezone. */

export const TIMEZONE = "Europe/Berlin";

const parts = new Intl.DateTimeFormat("en-CA", {
  timeZone: TIMEZONE,
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  weekday: "short",
  hourCycle: "h23",
});

const cache = new Map();

/**
 * Splits an instant into its local date, time of day and weekday.
 *
 * @param {number} epochSeconds
 * @returns {{ date: string, monthDay: string, minuteOfDay: number, weekend: boolean, month: number }}
 */
export function localTime(epochSeconds) {
  const key = Math.floor(epochSeconds / 60);
  const hit = cache.get(key);
  if (hit) return hit;
  const values = Object.fromEntries(
    parts
      .formatToParts(new Date(epochSeconds * 1000))
      .map((part) => [part.type, part.value]),
  );
  const result = {
    date: `${values.year}-${values.month}-${values.day}`,
    monthDay: `${values.month}-${values.day}`,
    minuteOfDay: Number(values.hour) * 60 + Number(values.minute),
    weekend: values.weekday === "Sat" || values.weekday === "Sun",
    month: Number(values.month),
  };
  if (cache.size > 200_000) cache.clear();
  cache.set(key, result);
  return result;
}
