// Mirrors of the Rust view types. Field casing follows serde on the Rust side:
// core types are snake_case, PromptView and GrantView are camelCase.
export type VaultStatus = "missing" | "locked" | "unlocked";
export type ApprovalMode = "per_run" | "time_boxed" | "remember";
export type GrantDuration = "min15" | "hour1" | "until_lock";

export interface EntryMeta {
  project: string;
  environment: string;
  name: string;
  kind: string;
  notes: string;
}

export interface SaveEntryArgs {
  project: string;
  environment: string;
  name: string;
  renameFrom: string | null;
  /** null keeps the stored value. */
  value: string | null;
  notes: string;
}

export interface PromptView {
  id: number;
  projectDir: string;
  command: string[];
  commandName: string;
  peerPid: number | null;
  peerExe: string | null;
  variables: [string, string][];
  references: string[];
  mode: ApprovalMode;
}

export type DecisionInput =
  | { allow: { mode: ApprovalMode; duration: GrantDuration } }
  | "deny";

export interface GrantView {
  hash: string;
  projectDir: string;
  commandName: string;
  references: string[];
  expiresAt: number | null;
  remembered: boolean;
}

export type AuditOutcome =
  | "approved"
  | "approved_by_grant"
  | "denied"
  | "timed_out"
  | "cancelled"
  | "rate_limited"
  | "busy"
  | "locked"
  | "entry_not_found"
  | "invalid";

export interface AuditRecord {
  at: number;
  project_dir: string;
  entries: string[];
  omitted: number;
  outcome: AuditOutcome;
}

export interface Settings {
  auto_lock_minutes: number;
}
