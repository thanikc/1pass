import { TestBed } from "@angular/core/testing";
import { describe, expect, it, vi } from "vitest";
import { VaultApi } from "../../api/vault-api";
import { SettingsComponent } from "./settings.component";

async function setup(changePassword = vi.fn().mockResolvedValue(undefined)) {
  const fake = { getSettings: vi.fn().mockResolvedValue({ auto_lock_minutes: 15 }), setAutoLock: vi.fn().mockResolvedValue(undefined), changePassword };
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: fake }] });
  const f = TestBed.createComponent(SettingsComponent);
  const el: HTMLElement = f.nativeElement;
  const tick = async () => {
    await new Promise((r) => setTimeout(r, 0));
    f.detectChanges();
  };
  f.detectChanges();
  await tick();
  const q = (n: string) => el.querySelector<HTMLInputElement>(`[name=${n}]`)!;
  const type = (n: string, v: string) => {
    q(n).value = v;
    q(n).dispatchEvent(new Event("input"));
    f.detectChanges();
  };
  const submit = async () => {
    el.querySelector("form")!.dispatchEvent(new Event("submit"));
    await tick();
  };
  return { el, fake, q, type, submit, tick, btn: () => el.querySelector<HTMLButtonElement>("button[type=submit]")! };
}

describe("SettingsComponent", () => {
  it("offers the fixed auto-lock choices and saves a selection", async () => {
    const { q, fake, tick, el } = await setup();
    const sel = q("autolock") as unknown as HTMLSelectElement;
    expect([...sel.options].map((o) => o.value)).toEqual(["1", "5", "15", "30", "60", "0"]);
    expect(el.textContent).toContain("Never");
    sel.value = "30";
    sel.dispatchEvent(new Event("change"));
    await tick();
    expect(fake.setAutoLock).toHaveBeenCalledWith(30);
  });

  it("requires current, a long-enough new password and a matching confirm", async () => {
    const { type, btn } = await setup();
    type("current", "old");
    type("next", "short");
    type("confirm", "short");
    expect(btn().disabled).toBe(true);
    type("next", "longenough1");
    type("confirm", "different1");
    expect(btn().disabled).toBe(true);
    type("confirm", "longenough1");
    expect(btn().disabled).toBe(false);
  });

  it("shows WrongPassword inline and clears the fields", async () => {
    const { type, submit, el, q } = await setup(vi.fn().mockRejectedValue("WrongPassword"));
    type("current", "old");
    type("next", "longenough1");
    type("confirm", "longenough1");
    await submit();
    expect(el.textContent).toContain("Wrong password.");
    expect([q("current").value, q("next").value, q("confirm").value]).toEqual(["", "", ""]);
  });

  it("clears the fields on success", async () => {
    const { type, submit, q, fake } = await setup();
    type("current", "old");
    type("next", "longenough1");
    type("confirm", "longenough1");
    await submit();
    expect(fake.changePassword).toHaveBeenCalledWith("old", "longenough1");
    expect(q("current").value).toBe("");
  });
});
