import { TestBed } from "@angular/core/testing";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EntryMeta } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { BrowserComponent } from "./browser.component";

const meta = (project: string, environment: string, name: string): EntryMeta => ({ project, environment, name, kind: "secret", notes: "" });
const ENTRIES = [meta("a", "dev", "ONE"), meta("a", "dev", "TWO"), meta("b", "prod", "THREE")];

async function setup(overrides: Record<string, unknown> = {}, entries = ENTRIES) {
  const fake = {
    listEnvironments: vi.fn().mockResolvedValue([...new Set(entries.map((e) => `${e.project}/${e.environment}`))].map((s) => s.split("/"))),
    listEntries: vi.fn().mockResolvedValue(entries),
    revealEntry: vi.fn(async (_p: string, _e: string, n: string) => `secret-of-${n}`),
    copyEntryValue: vi.fn().mockResolvedValue(undefined),
    copyReference: vi.fn().mockResolvedValue(undefined),
    deleteEntry: vi.fn().mockResolvedValue(undefined),
    createEnvironment: vi.fn().mockResolvedValue(undefined),
    renameEnvironment: vi.fn().mockResolvedValue(undefined),
    deleteEnvironment: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: fake }] });
  const f = TestBed.createComponent(BrowserComponent);
  const el: HTMLElement = f.nativeElement;
  const flush = async () => {
    await vi.advanceTimersByTimeAsync(0);
    f.detectChanges();
  };
  f.detectChanges();
  await flush();
  const click = async (sel: string, text?: string) => {
    const nodes = [...el.querySelectorAll<HTMLElement>(sel)];
    (text ? nodes.find((n) => n.textContent?.trim() === text)! : nodes[0]).click();
    await flush();
  };
  const value = () => el.querySelector("[data-testid=value]")?.textContent?.trim();
  return { f, el, fake, flush, click, value };
}

describe("BrowserComponent", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("groups projects and filters entries by environment", async () => {
    const { el, click } = await setup();
    expect(el.textContent).toContain("ONE");
    expect(el.textContent).not.toContain("THREE");
    await click("button.env", "prod");
    expect(el.textContent).toContain("THREE");
    expect(el.textContent).not.toContain("ONE");
  });

  it("renames the selected environment", async () => {
    const { el, fake, click, flush } = await setup();
    await click("[data-testid=rename-env]");
    const input = el.querySelector<HTMLInputElement>("input[name=rename]")!;
    input.value = "staging";
    input.dispatchEvent(new Event("input"));
    await flush();
    await click("button[type=submit]", "Save");
    expect(fake.renameEnvironment).toHaveBeenCalledWith("a", "dev", "staging");
  });

  it("deletes an environment only after confirmation", async () => {
    const { fake, click } = await setup();
    await click("[data-testid=delete-env]");
    expect(fake.deleteEnvironment).not.toHaveBeenCalled();
    await click("[data-testid=confirm-delete]");
    expect(fake.deleteEnvironment).toHaveBeenCalledWith("a", "dev");
  });

  it("masks by default and reveals only on request", async () => {
    const { click, value, fake } = await setup();
    await click("button.entry", "ONE");
    expect(value()).toBe("••••••••");
    expect(fake.revealEntry).not.toHaveBeenCalled();
    await click("[data-testid=reveal]");
    expect(value()).toBe("secret-of-ONE");
  });

  it("re-masks at exactly 10 s", async () => {
    const { click, value, flush } = await setup();
    await click("button.entry", "ONE");
    await click("[data-testid=reveal]");
    await vi.advanceTimersByTimeAsync(9999);
    await flush();
    expect(value()).toBe("secret-of-ONE");
    await vi.advanceTimersByTimeAsync(1);
    await flush();
    expect(value()).toBe("••••••••");
  });

  it("re-masks immediately when another entry is selected", async () => {
    const { click, value } = await setup();
    await click("button.entry", "ONE");
    await click("[data-testid=reveal]");
    await click("button.entry", "TWO");
    expect(value()).toBe("••••••••");
  });

  it("never shows a stale reveal under another entry", async () => {
    let release!: (v: string) => void;
    const slow = new Promise<string>((r) => (release = r));
    const { click, value, flush } = await setup({ revealEntry: vi.fn().mockReturnValueOnce(slow) });
    await click("button.entry", "ONE");
    await click("[data-testid=reveal]");
    await click("button.entry", "TWO");
    release("secret-of-ONE");
    await flush();
    expect(value()).toBe("••••••••");
  });

  it("copies without revealing", async () => {
    const { click, fake, el } = await setup();
    await click("button.entry", "ONE");
    await click("[data-testid=copy]");
    expect(fake.copyEntryValue).toHaveBeenCalledWith("a", "dev", "ONE");
    expect(fake.revealEntry).not.toHaveBeenCalled();
    expect(el.textContent).toContain("Clipboard clears in 30 s");
  });

  it("copies the reference", async () => {
    const { click, fake } = await setup();
    await click("button.entry", "ONE");
    await click("[data-testid=copyref]");
    expect(fake.copyReference).toHaveBeenCalledWith("a", "dev", "ONE");
  });

  it("asks for confirmation before deleting", async () => {
    const { click, fake } = await setup();
    await click("button.entry", "ONE");
    await click("[data-testid=delete]");
    expect(fake.deleteEntry).not.toHaveBeenCalled();
    await click("[data-testid=cancel-delete]");
    expect(fake.deleteEntry).not.toHaveBeenCalled();
    await click("[data-testid=delete]");
    await click("[data-testid=confirm-delete]");
    expect(fake.deleteEntry).toHaveBeenCalledWith("a", "dev", "ONE");
  });

  it("prompts to create a project when empty", async () => {
    const { el } = await setup({}, []);
    expect(el.textContent).toContain("Create a project");
  });

  it("clears the timer and value on destroy", async () => {
    const { click, f } = await setup();
    await click("button.entry", "ONE");
    await click("[data-testid=reveal]");
    f.destroy();
    expect(vi.getTimerCount()).toBe(0);
    expect(f.componentInstance.revealed()).toBeNull();
  });
});
