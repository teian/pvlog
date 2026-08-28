/**
 * Builds a spreadable prop object that omits the key entirely when no value is
 * available.
 *
 * Under `exactOptionalPropertyTypes` an optional prop may not receive an
 * explicit `undefined`; the key has to be absent instead. Spreading the result
 * of this helper keeps that decision out of the JSX and avoids repeating a
 * conditional per prop.
 *
 * @param key - Prop name to set when a value is present.
 * @param value - Value to assign, or `undefined` to omit the prop entirely.
 * @returns An object carrying the prop, or an empty object when absent.
 */
export function optionalProp<K extends string, V>(
  key: K,
  value: V | undefined,
): Partial<Record<K, V>> {
  return value === undefined ? {} : ({ [key]: value } as Partial<Record<K, V>>);
}
