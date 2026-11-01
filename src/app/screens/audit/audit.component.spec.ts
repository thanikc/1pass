import { TestBed } from "@angular/core/testing";
import { describe, expect, it, vi } from "vitest";
import { AuditRecord } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { AuditComponent } from "./audit.component";

async function setup(records: AuditRecord[]) {
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: { listAudit: vi.fn().mockResolvedValue(records) } }] });
  const f = TestBed.createComponent(AuditComponent);
  f.detectChanges();
  await new Promise((r) => setTimeout(r, 0));
  f.detectChanges();
  return f.nativeElement as HTMLElement;
}

const rec = (over: Partial<AuditRecord>): AuditRecord => ({ at: 1_700_000_000, project_dir: "/work/proj", entries: ["op://p/e/A"], omitted: 0, outcome: "approved", ...over });

describe("AuditComponent", () => {
  it("renders time, project, entries and outcome in the order given", async () => {
    const el = await setup([rec({ project_dir: "/first", outcome: "denied" }), rec({ project_dir: "/second" })]);
    const text = el.textContent!;
    expect(text).toContain("/first");
    expect(text.indexOf("/first")).toBeLessThan(text.indexOf("/second"));
    expect(text).toContain("Denied");
    expect(text).toContain("op://p/e/A");
    expect(text).toContain(new Date(1_700_000_000_000).toLocaleString());
  });

  it("shows +N more when entries were omitted", async () => {
    expect(await setup([rec({ omitted: 3 })])).toHaveProperty("textContent", expect.stringContaining("+3 more"));
  });

  it("shows an empty state", async () => {
    expect((await setup([])).textContent).toContain("Nothing has been requested yet");
  });
});
