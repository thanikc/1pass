import { TestBed } from "@angular/core/testing";
import { describe, expect, it, vi } from "vitest";
import { VaultApi } from "./api/vault-api";
import { VaultStatus } from "./api/types";
import { AppComponent } from "./app.component";

async function setup(initial: VaultStatus, ipcError: string | null = null) {
  let status = initial;
  let changed: () => void = () => {};
  const fake = {
    vaultStatus: vi.fn(async () => status),
    getIpcError: vi.fn(async () => ipcError),
    onVaultStateChanged: (cb: () => void) => {
      changed = cb;
      return () => {};
    },
  };
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: fake }] });
  const fixture = TestBed.createComponent(AppComponent);
  await fixture.whenStable();
  fixture.detectChanges();
  const el: HTMLElement = fixture.nativeElement;
  return {
    el,
    fixture,
    flip: async (s: VaultStatus) => {
      status = s;
      changed();
      await fixture.whenStable();
      fixture.detectChanges();
    },
  };
}

describe("AppComponent", () => {
  it("renders the unlock screen when locked", async () => {
    const { el } = await setup("locked");
    expect(el.querySelector("app-unlock")).toBeTruthy();
    expect(el.querySelector(".shell")).toBeNull();
  });

  it("renders the shell when unlocked and drops it when the state changes", async () => {
    const { el, flip } = await setup("unlocked");
    expect(el.querySelector(".shell")).toBeTruthy();
    await flip("locked");
    expect(el.querySelector(".shell")).toBeNull();
    expect(el.querySelector("app-unlock")).toBeTruthy();
  });

  it("shows a banner when the socket could not be bound", async () => {
    const { el } = await setup("locked", "another instance is running");
    expect(el.querySelector(".banner")?.textContent).toContain("cannot connect");
  });
});
