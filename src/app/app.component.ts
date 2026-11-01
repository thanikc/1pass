import { Component, OnDestroy, inject, signal } from "@angular/core";
import { VaultApi } from "./api/vault-api";
import { VaultStatus } from "./api/types";
import { BrowserComponent } from "./screens/browser/browser.component";
import { ApprovalDialogComponent } from "./screens/approvals/approval-dialog.component";
import { GrantsComponent } from "./screens/approvals/grants.component";
import { AuditComponent } from "./screens/audit/audit.component";
import { SettingsComponent } from "./screens/settings/settings.component";
import { UnlockComponent } from "./screens/unlock/unlock.component";

const ACTIVITY_THROTTLE_MS = 10_000;

@Component({
  selector: "app-root",
  imports: [UnlockComponent, BrowserComponent, GrantsComponent, ApprovalDialogComponent, AuditComponent, SettingsComponent],
  host: {
    "(document:mousedown)": "activity()",
    "(document:keydown)": "activity()",
  },
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnDestroy {
  private api = inject(VaultApi);
  private unlisten = this.api.onVaultStateChanged(() => void this.refresh());

  status = signal<VaultStatus | null>(null);
  tab = signal<"vault" | "approvals" | "audit" | "settings">("vault");
  private lastActivity = Number.NEGATIVE_INFINITY;
  ipcError = signal<string | null>(null);

  constructor() {
    void this.refresh();
    void this.api.getIpcError().then((e) => this.ipcError.set(e));
  }

  async refresh(): Promise<void> {
    this.status.set(await this.api.vaultStatus());
  }

  /** Tells the backend the user is active, at most once per 10 s. */
  activity(): void {
    const now = Date.now();
    if (now - this.lastActivity < ACTIVITY_THROTTLE_MS) return;
    this.lastActivity = now;
    void this.api.noteActivity();
  }

  async lock(): Promise<void> {
    await this.api.lock();
    await this.refresh();
  }

  ngOnDestroy(): void {
    this.unlisten();
  }
}
