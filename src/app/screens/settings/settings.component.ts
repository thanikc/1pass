import { Component, OnInit, computed, inject, signal } from "@angular/core";
import { errorMessage } from "../../api/errors";
import { VaultApi } from "../../api/vault-api";

const MIN_LENGTH = 8;

@Component({
  selector: "app-settings",
  templateUrl: "./settings.component.html",
  styleUrl: "./settings.component.css",
})
export class SettingsComponent implements OnInit {
  private api = inject(VaultApi);

  readonly choices: [number, string][] = [[1, "1 minute"], [5, "5 minutes"], [15, "15 minutes"], [30, "30 minutes"], [60, "1 hour"], [0, "Never"]];
  autoLock = signal(15);
  current = signal("");
  next = signal("");
  confirm = signal("");
  error = signal("");
  notice = signal("");
  cliTarget = signal<string | null>(null);
  cliError = signal("");
  cliNotice = signal("");

  canChange = computed(() => this.current().length > 0 && this.next().length >= MIN_LENGTH && this.next() === this.confirm());

  async ngOnInit(): Promise<void> {
    try {
      this.autoLock.set((await this.api.getSettings()).auto_lock_minutes);
    } catch (e) {
      this.error.set(errorMessage(e));
    }
    try {
      this.cliTarget.set(await this.api.cliInstallTarget());
    } catch (e) {
      this.cliError.set(errorMessage(e));
    }
  }

  async installCli(): Promise<void> {
    this.cliError.set("");
    this.cliNotice.set("");
    try {
      const path = await this.api.installCli();
      this.cliNotice.set(`Installed to ${path}. Open a new terminal to use it.`);
    } catch (e) {
      this.cliError.set(errorMessage(e));
    }
  }

  async setAutoLock(event: Event): Promise<void> {
    const minutes = Number((event.target as HTMLSelectElement).value);
    try {
      await this.api.setAutoLock(minutes);
      this.autoLock.set(minutes);
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  async changePassword(event: Event): Promise<void> {
    event.preventDefault();
    if (!this.canChange()) return;
    this.error.set("");
    this.notice.set("");
    try {
      await this.api.changePassword(this.current(), this.next());
      this.notice.set("Password changed.");
    } catch (e) {
      this.error.set(errorMessage(e));
    } finally {
      this.current.set("");
      this.next.set("");
      this.confirm.set("");
    }
  }

  text(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }
}
