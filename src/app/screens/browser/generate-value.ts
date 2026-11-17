const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
/** Largest multiple of the alphabet size that fits in a byte; bytes at or above it are rejected (no modulo bias). */
const LIMIT = 248;

export const GENERATED_LENGTH = 32;

/** Random alphanumeric string using rejection sampling over random bytes. */
export function generateValue(
  fill: (bytes: Uint8Array<ArrayBuffer>) => void = (b) => void crypto.getRandomValues(b),
  length = GENERATED_LENGTH,
): string {
  let out = "";
  while (out.length < length) {
    const bytes = new Uint8Array(length * 2);
    fill(bytes);
    for (const byte of bytes) {
      if (byte < LIMIT && out.length < length) out += ALPHABET[byte % ALPHABET.length];
    }
  }
  return out;
}
