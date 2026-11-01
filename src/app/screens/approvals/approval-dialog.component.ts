import { Component, ElementRef, OnDestroy, OnInit, computed, effect, inject, signal, untracked, viewChild } from "@angular/core";
import { ApprovalMode, GrantDuration, PromptView } from "../../api/types";
import { VaultApi } from "../../api/vault-api";

/** Allow stays disabled this long after a prompt appears, so a keystroke in progress cannot approve. */
export const ARM_DELAY_MS = 500;

@Component({
  selector: "app-approval-dialog",
  templateUrl: "./approval-dialog.component.html",
  styleUrl: "./approval-dialog.component.css",
})
export class ApprovalDialogComponent implements OnInit, OnDestroy {
  private api = inject(VaultApi);
  private unlisten: () => void = () => {};
  private deny = viewChild<ElementRef<HTMLButtonElement>>("deny");

  pending = signal<PromptView[]>([]);
  mode = signal<ApprovalMode>("time_boxed");
  duration = signal<GrantDuration>("min15");
  armed = signal(false);

  current = computed(() => this.pending()[0] ?? null);
  private currentId = computed(() => this.current()?.id);
  waiting = computed(() => Math.max(0, this.pending().length - 1));

  constructor() {
    effect((onCleanup) => {
      const id = this.currentId();
      this.armed.set(false);
      if (id === undefined) return;
      untracked(() => {
        this.mode.set(this.current()?.mode ?? "time_boxed");
        this.duration.set("min15");
      });
      const t = setTimeout(() => this.armed.set(true), ARM_DELAY_MS);
      onCleanup(() => clearTimeout(t));
      const opener = document.activeElement as HTMLElement | null;
      queueMicrotask(() => this.deny()?.nativeElement.focus());
      onCleanup(() => opener?.isConnected && queueMicrotask(() => opener.focus()));
    });
  }

  ngOnInit(): void {
    // Listen first, then fetch: a prompt that arrived earlier is still picked up.
    this.unlisten = this.api.onApprovalsChanged(() => void this.refresh());
    void this.refresh();
  }

  ngOnDestroy(): void {
    this.unlisten();
  }

  async refresh(): Promise<void> {
    this.pending.set(await this.api.listPendingApprovals());
  }

  async allow(): Promise<void> {
    if (this.armed()) await this.respond({ allow: { mode: this.mode(), duration: this.duration() } });
  }

  deny_(): Promise<void> {
    return this.respond("deny");
  }

  private async respond(decision: "deny" | { allow: { mode: ApprovalMode; duration: GrantDuration } }): Promise<void> {
    const c = this.current();
    if (!c) return;
    try {
      await this.api.respondApproval(c.id, decision);
    } catch {
      // Unknown id: it was already closed server-side. Just refresh.
    }
    await this.refresh();
  }

  /** Escape denies; Tab stays inside the dialog so the page behind is unreachable. */
  keydown(e: KeyboardEvent, dialog: HTMLElement): void {
    if (e.key === "Escape") {
      e.preventDefault();
      void this.deny_();
    } else if (e.key === "Tab") {
      const items = [...dialog.querySelectorAll<HTMLElement>("select, button:not(:disabled)")];
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      if (e.shiftKey && active === first) (e.preventDefault(), last.focus());
      else if (!e.shiftKey && active === last) (e.preventDefault(), first.focus());
    }
  }

  select(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }
}
