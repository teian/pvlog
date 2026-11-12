/**
 * Formats the stored integer of a channel as the number it stands for.
 *
 * @param value - Stored value, an integer with `scale` decimal places.
 * @param scale - Decimal places of the stored integer.
 * @param unit - Unit of the channel; may be empty.
 * @param locale - Locale to format the number in.
 * @returns The number without trailing zeros, followed by its unit.
 */
export function formatChannelValue(
  value: number,
  scale: number,
  unit: string,
  locale: string,
): string {
  const number = new Intl.NumberFormat(locale, {
    maximumFractionDigits: scale,
  }).format(value / 10 ** scale);
  return unit ? `${number} ${unit}` : number;
}
