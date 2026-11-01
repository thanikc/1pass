export type OnePassError =
  | "WrongPassword"
  | "VaultCorrupt"
  | "VaultLocked"
  | "Denied"
  | "RateLimited"
  | "Busy"
  | { EntryNotFound: string }
  | { VersionMismatch: { expected: number; got: number } }
  | { BadReference: string }
  | { InvalidInput: string }
  | { Io: string };

const FIXED: Record<string, string> = {
  WrongPassword: "Wrong password.",
  VaultCorrupt: "The vault file is damaged or has been tampered with.",
  VaultLocked: "The vault is locked.",
  Denied: "Request denied.",
  RateLimited: "Too many denied requests. Try again in a minute.",
  Busy: "Too many pending requests.",
};

const GENERIC = "Something went wrong.";

/** Never stringifies values it does not recognise. */
export function errorMessage(e: unknown): string {
  if (typeof e === "string") return FIXED[e] ?? GENERIC;
  if (typeof e !== "object" || e === null) return GENERIC;
  const o = e as Record<string, unknown>;
  if (typeof o["EntryNotFound"] === "string")
    return `Entry not found: ${o["EntryNotFound"]}`;
  if (typeof o["BadReference"] === "string")
    return `Invalid reference: ${o["BadReference"]}`;
  if (typeof o["InvalidInput"] === "string") return o["InvalidInput"];
  if (typeof o["Io"] === "string") return o["Io"];
  if (o["VersionMismatch"]) return "The app and op versions do not match.";
  return GENERIC;
}

/** The variant name of a Rust error, or null. */
export function errorKind(e: unknown): string | null {
  if (typeof e === "string") return e;
  if (typeof e === "object" && e !== null) return Object.keys(e)[0] ?? null;
  return null;
}
