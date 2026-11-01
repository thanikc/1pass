import { Component, OnInit, inject, signal } from "@angular/core";
import { errorMessage } from "../../api/errors";
import { AuditOutcome, AuditRecord } from "../../api/types";
import { VaultApi } from "../../api/vault-api";

const OUTCOMES: Record<AuditOutcome, string> = {
  approved: "Approved",
  approved_by_grant: "Approved (existing approval)",
  denied: "Denied",
  timed_out: "Timed out",
  cancelled: "Cancelled",
  rate_limited: "Rate limited",
  busy: "Busy",
  locked: "Vault locked",
  entry_not_found: "Entry not found",
  invalid: "Invalid request",
};

@Component({
  selector: "app-audit",
  templateUrl: "./audit.component.html",
  styleUrl: "./audit.component.css",
})
export class AuditComponent implements OnInit {
  private api = inject(VaultApi);
  records = signal<AuditRecord[]>([]);
  error = signal("");

  async ngOnInit(): Promise<void> {
    try {
      this.records.set(await this.api.listAudit());
    } catch (e) {
      this.error.set(errorMessage(e));
    }
  }

  when(r: AuditRecord): string {
    return new Date(r.at * 1000).toLocaleString();
  }

  outcome(r: AuditRecord): string {
    return OUTCOMES[r.outcome];
  }
}
