import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merges conditional classes while resolving Tailwind conflicts.
 *
 * @param inputs - Conditional class values.
 * @returns A normalized class string.
 */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
