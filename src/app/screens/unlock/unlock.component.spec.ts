import { TestBed } from "@angular/core/testing";
import { describe, expect, it, vi } from "vitest";
import { VaultApi } from "../../api/vault-api";
import { UnlockComponent } from "./unlock.component";

function setup(status: "missing" | "locked", api: Partial<Record<keyof VaultApi, unknown>> = {}) {
  const fake = { createVault: vi.fn().mockResolvedValue(undefined), unlock: vi.fn().mockResolvedValue(undefined), ...api };
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: fake }] });
  const fixture = TestBed.createComponent(UnlockComponent);
  fixture.componentRef.setInput("status", status);
  fixture.detectChanges();
  const el: HTMLElement = fixture.nativeElement;
  const input = (name: string) => el.querySelector<HTMLInputElement>(`input[name=${name}]`);
  const type = async (name: string, v: string) => {
    const i = input(name)!;
    i.value = v;
    i.dispatchEvent(new Event("input"));
    fixture.detectChanges();
  };
  const submit = async () => {
    el.querySelector("form")!.dispatchEvent(new Event("submit"));
    await fixture.whenStable();
    fixture.detectChanges();
  };
  const button = () => el.querySelector<HTMLButtonElement>("button[type=submit]");
  return { fake, el, input, type, submit, button, fixture };
}

describe("UnlockComponent", () => {
  it("shows the create form when missing", () => {
    const { input } = setup("missing");
    expect(input("password")).toBeTruthy();
    expect(input("confirm")).toBeTruthy();
  });

  it("blocks mismatched confirm", async () => {
    const { type, button, submit, fake } = setup("missing");
    await type("password", "password1");
    await type("confirm", "password2");
    expect(button()!.disabled).toBe(true);
    await submit();
    expect(fake.createVault).not.toHaveBeenCalled();
  });

  it("blocks passwords under 8 characters", async () => {
    const { type, button } = setup("missing");
    await type("password", "short");
    await type("confirm", "short");
    expect(button()!.disabled).toBe(true);
  });

  it("shows only the password field when locked", () => {
    const { input } = setup("locked");
    expect(input("password")).toBeTruthy();
    expect(input("confirm")).toBeNull();
  });

  it("shows the message and clears the input on a wrong password", async () => {
    const { type, submit, el, input } = setup("locked", { unlock: vi.fn().mockRejectedValue("WrongPassword") });
    await type("password", "nope");
    await submit();
    expect(el.textContent).toContain("Wrong password.");
    expect(input("password")!.value).toBe("");
  });

  it("clears the input after success", async () => {
    const { type, submit, input, fake } = setup("locked");
    await type("password", "password1");
    await submit();
    expect(fake.unlock).toHaveBeenCalledWith("password1");
    expect(input("password")!.value).toBe("");
  });

  it("offers no create action when the vault is corrupt", async () => {
    const { type, submit, el, input } = setup("locked", { unlock: vi.fn().mockRejectedValue("VaultCorrupt") });
    await type("password", "password1");
    await submit();
    expect(el.textContent).toContain("damaged");
    expect(input("password")).toBeNull();
    expect(el.textContent).not.toContain("Create");
  });
});
