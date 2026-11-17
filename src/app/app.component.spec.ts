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
    onApprovalsChanged: () => () => {},
    noteActivity: vi.fn(async () => {}),
    lock: vi.fn(async () => {}),
    listPendingApprovals: vi.fn(async () => []),
    listEnvironments: vi.fn(async () => []),
    listEntries: vi.fn(async () => []),
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
    fake,
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

  it("has the five tabs and a Lock button that calls lock", async () => {
    const { el, fake } = await setup("unlocked");
    const tabs = [...el.querySelectorAll("[role=tab]")].map((t) => t.textContent!.trim());
    expect(tabs).toEqual(["Vault", "Approvals", "Audit", "Settings", "Help"]);
    el.querySelector<HTMLButtonElement>("[data-testid=lock]")!.click();
    expect(fake.lock).toHaveBeenCalled();
  });

  it("moves between tabs with arrow keys and keeps one tab stop", async () => {
    const { el, fixture } = await setup("unlocked");
    const tablist = el.querySelector("[role=tablist]")!;
    tablist.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    fixture.detectChanges();
    const tabs = [...el.querySelectorAll<HTMLElement>("[role=tab]")];
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["false", "true", "false", "false", "false"]);
    expect(tabs.map((t) => t.tabIndex)).toEqual([-1, 0, -1, -1, -1]);
  });

  it("renders CLI usage instructions and an example in the Help tab", async () => {
    const { el, fixture } = await setup("unlocked");
    el.querySelector<HTMLButtonElement>("#tab-help")!.click();
    fixture.detectChanges();
    const panel = el.querySelector("#panel")!.textContent!;
    expect(panel).toContain("op://myapp/dev/DATABASE_URL");
    expect(panel).toContain("op run -- npm start");
  });

  it("Ctrl+F from another tab opens the Vault tab and focuses search", async () => {
    const { el, fixture } = await setup("unlocked");
    el.querySelector<HTMLButtonElement>("#tab-settings")!.click();
    fixture.detectChanges();
    const e = new KeyboardEvent("keydown", { key: "f", ctrlKey: true, cancelable: true });
    document.dispatchEvent(e);
    await fixture.whenStable();
    fixture.detectChanges();
    expect(e.defaultPrevented).toBe(true);
    expect(el.querySelector("#tab-vault")!.getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(el.querySelector("input[aria-label='Search entries']"));
  });

  it("Cmd+F also focuses search", async () => {
    const { el, fixture } = await setup("unlocked");
    el.querySelector<HTMLButtonElement>("#tab-help")!.click();
    fixture.detectChanges();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "F", metaKey: true, cancelable: true }));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(document.activeElement).toBe(el.querySelector("input[aria-label='Search entries']"));
  });

  it("Ctrl+L and Cmd+L lock the vault", async () => {
    const { fake } = await setup("unlocked");
    const e = new KeyboardEvent("keydown", { key: "l", ctrlKey: true, cancelable: true });
    document.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(true);
    expect(fake.lock).toHaveBeenCalledTimes(1);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "l", metaKey: true, cancelable: true }));
    expect(fake.lock).toHaveBeenCalledTimes(2);
  });

  it("shortcuts do nothing while locked", async () => {
    const { el, fake } = await setup("locked");
    for (const key of ["l", "f"]) {
      const e = new KeyboardEvent("keydown", { key, ctrlKey: true, cancelable: true });
      document.dispatchEvent(e);
      expect(e.defaultPrevented).toBe(false);
    }
    expect(fake.lock).not.toHaveBeenCalled();
    expect(el.querySelector(".shell")).toBeNull();
  });

  it("lists the shortcuts in the Help tab", async () => {
    const { el, fixture } = await setup("unlocked");
    el.querySelector<HTMLButtonElement>("#tab-help")!.click();
    fixture.detectChanges();
    const help = el.querySelector(".help")!;
    expect(help.textContent).toContain("Shortcuts");
    expect([...help.querySelectorAll("kbd")].map((k) => k.textContent)).toContain("F");
    expect(help.textContent).toContain("lock the vault");
  });

  it("reports activity at most once per 10 s", async () => {
    const { fake } = await setup("unlocked");
    vi.useFakeTimers();
    try {
      const fire = () => document.dispatchEvent(new Event("mousedown"));
      fire();
      fire();
      document.dispatchEvent(new Event("keydown"));
      expect(fake.noteActivity).toHaveBeenCalledTimes(1);
      vi.advanceTimersByTime(10_000);
      fire();
      expect(fake.noteActivity).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });
});
