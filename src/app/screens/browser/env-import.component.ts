import { Component, ElementRef, OnDestroy, computed, effect, inject, input, output, signal } from "@angular/core";
import { errorMessage } from "../../api/errors";
import { CandidateStatus, EnvCandidate } from "../../api/types";
import { VaultApi } from "../../api/vault-api";

const LABEL: Record<CandidateStatus, string> = {
  secret: "looks secret",
  literal: "literal",
  reference: "already a reference",
  empty: "empty value",
};

@Component({
  selector: "app-env-import",
  templateUrl: "./env-import.component.html",
})
export class EnvImportComponent implements OnDestroy {
  private api = inject(VaultApi);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private opener = document.activeElement as HTMLElement | null;

  project = input.required<string>();
  environment = input.required<string>();
  candidates = input.required<EnvCandidate[]>();
  /** Entries were stored: the browser should reload. */
  imported = output<void>();
  closed = output<void>();

  private ticks = signal<Record<string, boolean>>({});
  done = signal(false);
  error = signal("");
  notice = signal("");

  chosen = computed(() =>
    this.candidates()
      .filter((c) => this.importable(c) && (this.ticks()[c.name] ?? c.preselected))
      .map((c) => c.name)
      .sort(),
  );
  referenceLines = computed(() => {
    const [p, e] = [this.project(), this.environment()];
    return this.chosen().map((n) => `${n}=op://${p}/${e}/${n}`).join("\n");
  });

  constructor() {
    effect(() => {
      this.done();
      queueMicrotask(() => this.focusables()[0]?.focus());
    });
  }

  ngOnDestroy(): void {
    const o = this.opener;
    if (o?.isConnected) queueMicrotask(() => o.focus());
  }

  importable(c: EnvCandidate): boolean {
    return c.status === "secret" || c.status === "literal";
  }

  label(c: EnvCandidate): string {
    return LABEL[c.status];
  }

  reason(c: EnvCandidate): string {
    return c.status === "reference" ? "Already an op:// reference." : "Empty values cannot be imported.";
  }

  isChecked(c: EnvCandidate): boolean {
    return this.chosen().includes(c.name);
  }

  toggle(name: string, event: Event): void {
    const on = (event.target as HTMLInputElement).checked;
    this.ticks.update((t) => ({ ...t, [name]: on }));
  }

  async doImport(): Promise<void> {
    if (!this.chosen().length) return;
    this.error.set("");
    try {
      await this.api.importEnv(this.project(), this.environment(), this.chosen());
      this.done.set(true);
      this.imported.emit();
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  async copy(): Promise<void> {
    this.error.set("");
    this.notice.set("");
    try {
      await this.api.copyEnvReferences(this.project(), this.environment(), this.chosen());
      this.notice.set("Reference lines copied.");
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  private focusables(): HTMLElement[] {
    return [...this.host.nativeElement.querySelectorAll<HTMLElement>("input:not(:disabled), button:not(:disabled)")];
  }

  /** Escape cancels; Tab stays inside the dialog. */
  keydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      this.closed.emit();
    } else if (e.key === "Tab") {
      const items = this.focusables();
      const [first, last] = [items[0], items[items.length - 1]];
      const active = document.activeElement;
      if (e.shiftKey && active === first) (e.preventDefault(), last.focus());
      else if (!e.shiftKey && active === last) (e.preventDefault(), first.focus());
    }
  }
}
