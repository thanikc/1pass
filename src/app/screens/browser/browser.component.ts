import { Component, ElementRef, OnDestroy, OnInit, computed, effect, inject, signal, viewChild } from "@angular/core";
import { errorMessage } from "../../api/errors";
import { EntryMeta } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { EntryEditorComponent } from "./entry-editor.component";

const REVEAL_MS = 10_000;

@Component({
  selector: "app-browser",
  imports: [EntryEditorComponent],
  templateUrl: "./browser.component.html",
  styleUrl: "./browser.component.css",
})
export class BrowserComponent implements OnInit, OnDestroy {
  private api = inject(VaultApi);
  private timer: ReturnType<typeof setTimeout> | undefined;
  private revealSeq = 0;

  envs = signal<[string, string][]>([]);
  entries = signal<EntryMeta[]>([]);
  selectedEnv = signal<[string, string] | null>(null);
  selectedName = signal<string | null>(null);
  revealed = signal<string | null>(null);
  editing = signal<"new" | "edit" | null>(null);
  confirm = signal<"entry" | "env" | null>(null);
  private cancelBtn = viewChild<ElementRef<HTMLButtonElement>>("cancel");
  notice = signal("");
  error = signal("");
  newProject = signal("");
  newEnv = signal("");
  renaming = signal(false);
  renameValue = signal("");

  constructor() {
    effect((onCleanup) => {
      if (!this.confirm()) return;
      const opener = document.activeElement as HTMLElement | null;
      queueMicrotask(() => this.cancelBtn()?.nativeElement.focus());
      onCleanup(() => opener?.isConnected && queueMicrotask(() => opener.focus()));
    });
  }

  projects = computed(() => {
    const m = new Map<string, string[]>();
    for (const [p, e] of this.envs()) m.set(p, [...(m.get(p) ?? []), e]);
    return [...m.entries()];
  });
  visible = computed(() => {
    const s = this.selectedEnv();
    return s ? this.entries().filter((e) => e.project === s[0] && e.environment === s[1]) : [];
  });
  selected = computed(() => this.visible().find((e) => e.name === this.selectedName()) ?? null);

  ngOnInit(): void {
    void this.load();
  }

  ngOnDestroy(): void {
    this.hide();
  }

  async load(): Promise<void> {
    try {
      const [envs, entries] = await Promise.all([this.api.listEnvironments(), this.api.listEntries()]);
      this.envs.set(envs);
      this.entries.set(entries);
      if (!this.selectedEnv() && envs.length) this.selectedEnv.set(envs[0]);
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  private hide(): void {
    clearTimeout(this.timer);
    this.revealSeq++;
    this.revealed.set(null);
  }

  selectEnv(p: string, e: string): void {
    this.renaming.set(false);
    this.selectedEnv.set([p, e]);
    this.select(null);
  }

  select(name: string | null): void {
    this.hide();
    this.selectedName.set(name);
    this.editing.set(null);
    this.notice.set("");
  }

  async reveal(): Promise<void> {
    const e = this.selected();
    if (!e) return;
    const seq = ++this.revealSeq;
    try {
      const v = await this.api.revealEntry(e.project, e.environment, e.name);
      if (seq !== this.revealSeq) return; // selection changed while the request was in flight
      this.revealed.set(v);
      this.timer = setTimeout(() => this.hide(), REVEAL_MS);
    } catch (err) {
      this.error.set(errorMessage(err));
    }
  }

  private async run(action: () => Promise<void>, notice = ""): Promise<void> {
    this.error.set("");
    try {
      await action();
      this.notice.set(notice);
    } catch (err) {
      this.error.set(errorMessage(err));
    }
  }

  copyValue(): Promise<void> {
    const e = this.selected();
    return e ? this.run(() => this.api.copyEntryValue(e.project, e.environment, e.name), "Copied. Clipboard clears in 30 s.") : Promise.resolve();
  }

  copyRef(): Promise<void> {
    const e = this.selected();
    return e ? this.run(() => this.api.copyReference(e.project, e.environment, e.name), "Reference copied.") : Promise.resolve();
  }

  async remove(): Promise<void> {
    const e = this.selected();
    if (!e) return;
    this.confirm.set(null);
    await this.run(() => this.api.deleteEntry(e.project, e.environment, e.name));
    this.select(null);
    await this.load();
  }

  async createEnv(event: Event): Promise<void> {
    event.preventDefault();
    const [p, e] = [this.newProject(), this.newEnv()];
    await this.run(() => this.api.createEnvironment(p, e));
    if (this.error()) return;
    this.newProject.set("");
    this.newEnv.set("");
    await this.load();
    this.selectEnv(p, e);
  }

  startRename(): void {
    this.renameValue.set(this.selectedEnv()?.[1] ?? "");
    this.renaming.set(true);
  }

  async renameEnv(event: Event): Promise<void> {
    event.preventDefault();
    const s = this.selectedEnv();
    if (!s) return;
    const to = this.renameValue();
    await this.run(() => this.api.renameEnvironment(s[0], s[1], to));
    if (this.error()) return;
    this.renaming.set(false);
    await this.load();
    this.selectEnv(s[0], to);
  }

  async removeEnv(): Promise<void> {
    const s = this.selectedEnv();
    if (!s) return;
    this.confirm.set(null);
    await this.run(() => this.api.deleteEnvironment(s[0], s[1]));
    if (this.error()) return;
    this.selectedEnv.set(null);
    this.select(null);
    await this.load();
  }

  /** Escape cancels; Tab stays inside the dialog. */
  confirmKey(e: KeyboardEvent, dialog: HTMLElement): void {
    if (e.key === "Escape") {
      e.preventDefault();
      this.confirm.set(null);
    } else if (e.key === "Tab") {
      const [first, last] = [dialog.querySelector("button")!, dialog.querySelector("button:last-of-type")!] as HTMLElement[];
      const active = document.activeElement;
      if (e.shiftKey && active === first) (e.preventDefault(), last.focus());
      else if (!e.shiftKey && active === last) (e.preventDefault(), first.focus());
    }
  }

  async saved(name: string): Promise<void> {
    await this.load();
    this.select(name);
  }

  text(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }
}
