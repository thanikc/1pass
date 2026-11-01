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

function initialTheme(): "light" | "dark" {
  try {
    const t = localStorage.getItem("theme");
    if (t === "light" || t === "dark") return t;
  } catch {}
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

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
  readonly tabs = [
    ["vault", "Vault", "M4 4h16v15H4z M7 19v2 M17 19v2 M8.5 11.5a3.5 3.5 0 1 0 7 0 3.5 3.5 0 1 0-7 0 M12 5.5v2 M12 15.5v2 M6.5 11.5h2 M15.5 11.5h2"],
    ["approvals", "Approvals", "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z M9 12l2 2 4-4"],
    ["audit", "Audit", "M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01"],
    ["settings", "Settings", "M4 6h10 M18 6h2 M4 12h4 M12 12h8 M4 18h12 M14 4v4 M8 10v4 M16 16v4"],
  ] as const;
  collapsed = signal(false);
  theme = signal<"light" | "dark">(initialTheme());
  tab = signal<(typeof this.tabs)[number][0]>("vault");
  private lastActivity = Number.NEGATIVE_INFINITY;
  ipcError = signal<string | null>(null);

  constructor() {
    document.documentElement.dataset["theme"] = this.theme();
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

  /** Arrow/Home/End move between tabs (ARIA tabs pattern). */
  tabKey(e: KeyboardEvent): void {
    const i = this.tabs.findIndex((t) => t[0] === this.tab());
    const n = this.tabs.length;
    const next = { ArrowRight: (i + 1) % n, ArrowDown: (i + 1) % n, ArrowLeft: (i + n - 1) % n, ArrowUp: (i + n - 1) % n, Home: 0, End: n - 1 }[e.key];
    if (next === undefined) return;
    e.preventDefault();
    this.tab.set(this.tabs[next][0]);
    document.getElementById("tab-" + this.tabs[next][0])?.focus();
  }

  toggleTheme(): void {
    const t = this.theme() === "dark" ? "light" : "dark";
    this.theme.set(t);
    document.documentElement.dataset["theme"] = t;
    try {
      localStorage.setItem("theme", t);
    } catch {}
  }

  async lock(): Promise<void> {
    await this.api.lock();
    await this.refresh();
  }

  ngOnDestroy(): void {
    this.unlisten();
  }
}
