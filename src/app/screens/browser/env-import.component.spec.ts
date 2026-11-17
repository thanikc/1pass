import { TestBed } from "@angular/core/testing";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EnvCandidate } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { EnvImportComponent } from "./env-import.component";

const CANDIDATES: EnvCandidate[] = [
  { name: "TOKEN", status: "secret", replaces: false, preselected: true },
  { name: "HOST", status: "literal", replaces: false, preselected: false },
  { name: "OLD", status: "secret", replaces: true, preselected: false },
  { name: "REF", status: "reference", replaces: false, preselected: false },
  { name: "BLANK", status: "empty", replaces: false, preselected: false },
];

function setup(overrides: Record<string, unknown> = {}) {
  const fake = {
    importEnv: vi.fn().mockResolvedValue(undefined),
    copyEnvReferences: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: fake }] });
  const f = TestBed.createComponent(EnvImportComponent);
  f.componentRef.setInput("project", "a");
  f.componentRef.setInput("environment", "dev");
  f.componentRef.setInput("candidates", CANDIDATES);
  const imported = vi.fn();
  const closed = vi.fn();
  f.componentInstance.imported.subscribe(imported);
  f.componentInstance.closed.subscribe(closed);
  const el: HTMLElement = f.nativeElement;
  const flush = async () => {
    await vi.advanceTimersByTimeAsync(0);
    f.detectChanges();
  };
  f.detectChanges();
  const box = (name: string) => el.querySelector<HTMLInputElement>(`input[type=checkbox][name="${name}"]`)!;
  const click = async (sel: string) => {
    el.querySelector<HTMLElement>(sel)!.click();
    await flush();
  };
  return { f, el, fake, imported, closed, flush, box, click };
}

describe("EnvImportComponent", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("is a labelled modal dialog", () => {
    const { el } = setup();
    const d = el.querySelector("[role=dialog]")!;
    expect(d.getAttribute("aria-modal")).toBe("true");
    expect(el.querySelector("#" + d.getAttribute("aria-labelledby"))?.textContent).toContain("Import");
  });

  it("preselects only preselected rows", () => {
    const { box } = setup();
    expect(box("TOKEN").checked).toBe(true);
    expect(box("HOST").checked).toBe(false);
    expect(box("OLD").checked).toBe(false);
  });

  it("disables reference and empty rows", () => {
    const { box } = setup();
    expect(box("REF").disabled).toBe(true);
    expect(box("BLANK").disabled).toBe(true);
    expect(box("TOKEN").disabled).toBe(false);
  });

  it("warns on rows that replace an existing entry", () => {
    const { el } = setup();
    const rows = [...el.querySelectorAll("li")];
    expect(rows.find((r) => r.textContent!.includes("OLD"))!.textContent).toContain("replaced");
    expect(rows.find((r) => r.textContent!.includes("TOKEN"))!.textContent).not.toContain("replaced");
  });

  it("imports the chosen names", async () => {
    const { fake, box, flush, click, imported } = setup();
    box("OLD").click();
    await flush();
    await click("[data-testid=import]");
    expect(fake.importEnv).toHaveBeenCalledTimes(1);
    const [p, e, names] = fake.importEnv.mock.calls[0];
    expect([p, e]).toEqual(["a", "dev"]);
    expect([...names].sort()).toEqual(["OLD", "TOKEN"]);
    expect(imported).toHaveBeenCalled();
  });

  it("disables Import when nothing is chosen", async () => {
    const { el, box, flush } = setup();
    box("TOKEN").click();
    await flush();
    expect(el.querySelector<HTMLButtonElement>("[data-testid=import]")!.disabled).toBe(true);
  });

  it("shows the error and stays on the checklist", async () => {
    const { el, click } = setup({ importEnv: vi.fn().mockRejectedValue({ InvalidInput: "file changed" }) });
    await click("[data-testid=import]");
    expect(el.querySelector("[role=alert]")!.textContent).toContain("file changed");
    expect(el.querySelector("input[type=checkbox]")).toBeTruthy();
  });

  it("shows sorted reference lines after import and copies them", async () => {
    const { el, fake, box, flush, click, closed } = setup();
    box("HOST").click();
    await flush();
    await click("[data-testid=import]");
    expect(el.querySelector("pre")!.textContent!.trim()).toBe("HOST=op://a/dev/HOST\nTOKEN=op://a/dev/TOKEN");
    await click("[data-testid=copy-refs]");
    expect(fake.copyEnvReferences).toHaveBeenCalledWith("a", "dev", ["HOST", "TOKEN"]);
    await click("[data-testid=done]");
    expect(closed).toHaveBeenCalled();
  });

  it("cancel closes without calling the api", async () => {
    const { fake, click, closed } = setup();
    await click("[data-testid=cancel]");
    expect(closed).toHaveBeenCalled();
    expect(fake.importEnv).not.toHaveBeenCalled();
  });

  it("Escape cancels and Tab cycles inside the dialog", async () => {
    const { el, closed } = setup();
    const d = el.querySelector<HTMLElement>("[role=dialog]")!;
    const items = [...d.querySelectorAll<HTMLElement>("input:not(:disabled), button:not(:disabled)")];
    items[items.length - 1].focus();
    const tab = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    d.dispatchEvent(tab);
    expect(tab.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(items[0]);
    d.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(closed).toHaveBeenCalled();
  });
});
