import { Component, OnDestroy, inject, signal } from "@angular/core";
import { VaultApi } from "./api/vault-api";
import { VaultStatus } from "./api/types";
import { BrowserComponent } from "./screens/browser/browser.component";
import { ApprovalDialogComponent } from "./screens/approvals/approval-dialog.component";
import { GrantsComponent } from "./screens/approvals/grants.component";
import { UnlockComponent } from "./screens/unlock/unlock.component";

@Component({
  selector: "app-root",
  imports: [UnlockComponent, BrowserComponent, GrantsComponent, ApprovalDialogComponent],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnDestroy {
  private api = inject(VaultApi);
  private unlisten = this.api.onVaultStateChanged(() => void this.refresh());

  status = signal<VaultStatus | null>(null);
  tab = signal<"vault" | "approvals">("vault");
  ipcError = signal<string | null>(null);

  constructor() {
    void this.refresh();
    void this.api.getIpcError().then((e) => this.ipcError.set(e));
  }

  async refresh(): Promise<void> {
    this.status.set(await this.api.vaultStatus());
  }

  ngOnDestroy(): void {
    this.unlisten();
  }
}
