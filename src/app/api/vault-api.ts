import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ApprovalMode,
  AuditRecord,
  DecisionInput,
  EntryMeta,
  GrantView,
  PromptView,
  SaveEntryArgs,
  Settings,
  VaultStatus,
} from "./types";

/** The only place that calls `invoke`. */
@Injectable({ providedIn: "root" })
export class VaultApi {
  vaultStatus = () => invoke<VaultStatus>("vault_status");
  createVault = (password: string) => invoke<void>("create_vault", { password });
  unlock = (password: string) => invoke<void>("unlock", { password });
  lock = () => invoke<void>("lock");
  changePassword = (current: string, next: string) =>
    invoke<void>("change_password", { current, next });

  listEntries = () => invoke<EntryMeta[]>("list_entries");
  listEnvironments = () => invoke<[string, string][]>("list_environments");
  createEnvironment = (project: string, environment: string) =>
    invoke<void>("create_environment", { project, environment });
  saveEntry = (a: SaveEntryArgs) => invoke<void>("save_entry", { ...a });
  deleteEntry = (project: string, environment: string, name: string) =>
    invoke<void>("delete_entry", { project, environment, name });
  renameEnvironment = (project: string, from: string, to: string) =>
    invoke<void>("rename_environment", { project, from, to });
  deleteEnvironment = (project: string, environment: string) =>
    invoke<void>("delete_environment", { project, environment });
  revealEntry = (project: string, environment: string, name: string) =>
    invoke<string>("reveal_entry", { project, environment, name });
  copyEntryValue = (project: string, environment: string, name: string) =>
    invoke<void>("copy_entry_value", { project, environment, name });
  copyReference = (project: string, environment: string, name: string) =>
    invoke<void>("copy_reference", { project, environment, name });

  listPendingApprovals = () => invoke<PromptView[]>("list_pending_approvals");
  respondApproval = (id: number, decision: DecisionInput) =>
    invoke<void>("respond_approval", { id, decision });
  listGrants = () => invoke<GrantView[]>("list_grants");
  revokeGrant = (hash: string) => invoke<void>("revoke_grant", { hash });
  listProjectModes = () =>
    invoke<Record<string, ApprovalMode>>("list_project_modes");
  setProjectMode = (projectDir: string, mode: ApprovalMode) =>
    invoke<void>("set_project_mode", { projectDir, mode });

  listAudit = () => invoke<AuditRecord[]>("list_audit");
  getSettings = () => invoke<Settings>("get_settings");
  setAutoLock = (minutes: number) => invoke<void>("set_auto_lock", { minutes });
  noteActivity = () => invoke<void>("note_activity");
  getIpcError = () => invoke<string | null>("get_ipc_error");
  cliInstallTarget = () => invoke<string | null>("cli_install_target");
  installCli = () => invoke<string>("install_cli");

  onVaultStateChanged = (cb: () => void) => this.on("vault-state-changed", cb);
  onApprovalsChanged = (cb: () => void) => this.on("approvals-changed", cb);

  private on(event: string, cb: () => void): () => void {
    const unlisten = listen(event, () => cb());
    return () => void unlisten.then((f) => f());
  }
}
