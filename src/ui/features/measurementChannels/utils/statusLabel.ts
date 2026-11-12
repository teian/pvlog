import type { SourceProfile } from "@/entities/dataSource";

const placeholder = /\{(serial|phase|tracker)\}/g;

/** Turns a key pattern such as `inv.{serial}.status` into an expression that matches its keys. */
function keyExpression(pattern: string): RegExp {
  const parts = pattern.split(placeholder);
  // `split` with a capturing group alternates literal text and placeholder names.
  const source = parts
    .map((part, index) =>
      index % 2 === 0
        ? part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
        : part === "serial"
          ? "[^.]+"
          : "\\d+",
    )
    .join("");
  return new RegExp(`^${source}$`);
}

/**
 * Finds what a status code of a channel means, as the source profiles define it.
 *
 * @param channelKey - Key of the channel, such as `inv.2000123456.status`.
 * @param code - Code the channel reported.
 * @param profiles - Source profiles with their channel keys.
 * @returns The label of the code, or null when no profile labels it.
 */
export function statusLabel(
  channelKey: string,
  code: number,
  profiles: SourceProfile[],
): string | null {
  for (const profile of profiles)
    for (const key of profile.channelKeys) {
      if (!key.statusLabels || !keyExpression(key.key).test(channelKey))
        continue;
      const match = key.statusLabels.find((entry) => entry.code === code);
      if (match) return match.label;
    }
  return null;
}
