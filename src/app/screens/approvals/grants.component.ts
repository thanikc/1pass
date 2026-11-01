import { Component, OnInit, inject, signal } from "@angular/core";
import { errorMessage } from "../../api/errors";
import { ApprovalMode, GrantView } from "../../api/types";
import { VaultApi } from "../../api/vault-api";

@Component({
  selector: "app-grants",
  templateUrl: "./grants.component.html",
  styleUrl: "./grants.component.css",
})
export class GrantsComponent implements OnInit {
  private api = inject(VaultApi);
  grants = signal<GrantView[]>([]);
  modes = signal<[string, ApprovalMode][]>([]);
  error = signal("");

  ngOnInit(): void {
    void this.load();
  }

  async load(): Promise<void> {
    try {
      const [g, m] = await Promise.all([this.api.listGrants(), this.api.listProjectModes()]);
      this.grants.set(g);
      this.modes.set(Object.entries(m));
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  expiry(g: GrantView): string {
    if (g.remembered) return "Remembered";
    return g.expiresAt === null ? "Until lock" : `Expires ${new Date(g.expiresAt * 1000).toLocaleTimeString()}`;
  }

  async revoke(hash: string): Promise<void> {
    try {
      await this.api.revokeGrant(hash);
    } catch (e) {
      this.error.set(errorMessage(e));
    }
    await this.load();
  }

  async setMode(dir: string, event: Event): Promise<void> {
    try {
      await this.api.setProjectMode(dir, (event.target as HTMLSelectElement).value as ApprovalMode);
    } catch (e) {
      this.error.set(errorMessage(e));
    }
    await this.load();
  }
}
