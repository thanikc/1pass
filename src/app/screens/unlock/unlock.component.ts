import { Component, computed, inject, input, output, signal } from "@angular/core";
import { errorKind, errorMessage } from "../../api/errors";
import { VaultApi } from "../../api/vault-api";

const MIN_LENGTH = 8;

@Component({
  selector: "app-unlock",
  templateUrl: "./unlock.component.html",
  styleUrl: "./unlock.component.css",
})
export class UnlockComponent {
  private api = inject(VaultApi);

  /** `missing` shows the create form, `locked` the unlock form. */
  status = input.required<"missing" | "locked">();
  unlocked = output<void>();

  password = signal("");
  confirm = signal("");
  error = signal("");
  corrupt = signal(false);
  busy = signal(false);

  creating = computed(() => this.status() === "missing");
  valid = computed(() =>
    this.creating()
      ? this.password().length >= MIN_LENGTH && this.password() === this.confirm()
      : this.password().length > 0,
  );

  async submit(event: Event): Promise<void> {
    event.preventDefault();
    if (!this.valid() || this.busy()) return;
    this.busy.set(true);
    this.error.set("");
    try {
      if (this.creating()) await this.api.createVault(this.password());
      else await this.api.unlock(this.password());
      this.unlocked.emit();
    } catch (e) {
      this.error.set(errorMessage(e));
      this.corrupt.set(errorKind(e) === "VaultCorrupt");
    } finally {
      this.password.set("");
      this.confirm.set("");
      this.busy.set(false);
    }
  }

  value(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }
}
