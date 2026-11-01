import { TestBed } from "@angular/core/testing";
import { describe, expect, it, vi } from "vitest";
import { GrantView } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { GrantsComponent } from "./grants.component";

const grant = (over: Partial<GrantView>): GrantView => ({
  hash: "h1",
  projectDir: "/work/proj",
  commandName: "node",
  references: ["op://p/e/K"],
  expiresAt: null,
  remembered: false,
  ...over,
});

const tick = async (f: { detectChanges(): void }) => {
  await new Promise((r) => setTimeout(r, 0));
  f.detectChanges();
};

async function setup(grants: GrantView[]) {
  const fake = {
    listGrants: vi.fn().mockResolvedValue(grants),
    revokeGrant: vi.fn().mockResolvedValue(undefined),
    listProjectModes: vi.fn().mockResolvedValue({ "/work/proj": "time_boxed" }),
    setProjectMode: vi.fn().mockResolvedValue(undefined),
  };
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: fake }] });
  const f = TestBed.createComponent(GrantsComponent);
  f.detectChanges();
  await tick(f);
  return { f, el: f.nativeElement as HTMLElement, fake, tick: () => tick(f) };
}

describe("GrantsComponent", () => {
  it("lists grants with project, command, entries and expiry label", async () => {
    const { el } = await setup([grant({}), grant({ hash: "h2", remembered: true }), grant({ hash: "h3", expiresAt: 4_000_000_000 })]);
    expect(el.textContent).toContain("node");
    expect(el.textContent).toContain("/work/proj");
    expect(el.textContent).toContain("op://p/e/K");
    expect(el.textContent).toContain("Until lock");
    expect(el.textContent).toContain("Remembered");
    expect(el.textContent).toContain("Expires");
  });

  it("revokes and refreshes", async () => {
    const { el, fake, tick } = await setup([grant({})]);
    el.querySelector<HTMLButtonElement>("[data-testid=revoke]")!.click();
    await tick();
    expect(fake.revokeGrant).toHaveBeenCalledWith("h1");
    expect(fake.listGrants).toHaveBeenCalledTimes(2);
  });

  it("changes a project's mode and warns that it revokes grants", async () => {
    const { el, fake, tick } = await setup([]);
    expect(el.textContent).toContain("revokes");
    const sel = el.querySelector<HTMLSelectElement>(".mode select")!;
    sel.value = "per_run";
    sel.dispatchEvent(new Event("change"));
    await tick();
    expect(fake.setProjectMode).toHaveBeenCalledWith("/work/proj", "per_run");
  });
});
