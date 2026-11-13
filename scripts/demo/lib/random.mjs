/**
 * Deterministic pseudo-random numbers, so that demo data built twice from the
 * same source is the same.
 */

/** Hashes a text to a 32-bit seed (FNV-1a). @param {string} text @returns {number} */
export function seedOf(text) {
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash;
}

/** Returns a generator of numbers in [0, 1) (mulberry32). @param {number} seed @returns {() => number} */
export function generator(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4_294_967_296;
  };
}

/** A generator seeded from text. @param {string} text @returns {() => number} */
export function seeded(text) {
  return generator(seedOf(text));
}
