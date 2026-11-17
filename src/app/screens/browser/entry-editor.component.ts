import { Component, OnDestroy, computed, inject, input, output, signal } from "@angular/core";
import { errorMessage } from "../../api/errors";
import { EntryMeta } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { generateValue } from "./generate-value";

const NAME_RE = /^[A-Za-z0-9._-]{1,128}$/;
const TRAILING_BREAKS_RE = /[\r\n]+$/;

@Component({
  selector: "app-entry-editor",
  templateUrl: "./entry-editor.component.html",
  styleUrl: "./entry-editor.component.css",
})
export class EntryEditorComponent implements OnDestroy {
  private api = inject(VaultApi);

  project = input.required<string>();
  environment = input.required<string>();
  /** null creates a new entry. */
  entry = input<EntryMeta | null>(null);
  saved = output<string>();
  cancelled = output<void>();

  private nameEdit = signal<string | null>(null);
  private noteEdit = signal<string | null>(null);
  value = signal("");
  error = signal("");

  name = computed(() => this.nameEdit() ?? this.entry()?.name ?? "");
  notes = computed(() => this.noteEdit() ?? this.entry()?.notes ?? "");
  nameValid = computed(() => NAME_RE.test(this.name()));
  renaming = computed(() => !!this.entry() && this.name() !== this.entry()!.name);
  /** The value to store: trailing line breaks removed, inner ones kept. */
  private stored = computed(() => this.value().replace(TRAILING_BREAKS_RE, ""));
  canSave = computed(() => this.nameValid() && (!!this.entry() || this.stored().length > 0));

  ngOnDestroy(): void {
    this.value.set("");
  }

  async save(event: Event): Promise<void> {
    event.preventDefault();
    if (!this.canSave()) return;
    try {
      await this.api.saveEntry({
        project: this.project(),
        environment: this.environment(),
        name: this.name(),
        renameFrom: this.renaming() ? this.entry()!.name : null,
        value: this.stored() === "" ? null : this.stored(),
        notes: this.notes(),
      });
      this.saved.emit(this.name());
    } catch (e) {
      this.error.set(errorMessage(e));
    } finally {
      this.value.set("");
    }
  }

  text(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  setName(e: Event): void {
    this.nameEdit.set(this.text(e));
  }

  setNotes(e: Event): void {
    this.noteEdit.set(this.text(e));
  }

  setValue(e: Event): void {
    this.value.set(this.text(e));
  }

  generate(): void {
    this.value.set(generateValue());
  }
}
