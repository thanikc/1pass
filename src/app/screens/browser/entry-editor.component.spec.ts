import { TestBed } from "@angular/core/testing";
import { describe, expect, it, vi } from "vitest";
import { EntryMeta } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { EntryEditorComponent } from "./entry-editor.component";

const META: EntryMeta = { project: "p", environment: "e", name: "OLD", kind: "secret", notes: "n" };

function setup(entry: EntryMeta | null = null, saveEntry = vi.fn().mockResolvedValue(undefined)) {
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: { saveEntry } }] });
  const f = TestBed.createComponent(EntryEditorComponent);
  f.componentRef.setInput("project", "p");
  f.componentRef.setInput("environment", "e");
  f.componentRef.setInput("entry", entry);
  f.detectChanges();
  const el: HTMLElement = f.nativeElement;
  const q = (n: string) => el.querySelector<HTMLInputElement>(`[name=${n}]`)!;
  const type = (n: string, v: string) => {
    q(n).value = v;
    q(n).dispatchEvent(new Event("input"));
    f.detectChanges();
  };
  const save = async () => {
    el.querySelector("form")!.dispatchEvent(new Event("submit"));
    await f.whenStable();
    f.detectChanges();
  };
  const btn = () => el.querySelector<HTMLButtonElement>("button[type=submit]")!;
  return { f, el, q, type, save, btn, saveEntry };
}

describe("EntryEditorComponent", () => {
  it("requires name and value for a new entry", () => {
    const { type, btn } = setup();
    expect(btn().disabled).toBe(true);
    type("name", "KEY");
    expect(btn().disabled).toBe(true);
    type("value", "v");
    expect(btn().disabled).toBe(false);
  });

  it("generates a 32 character alphanumeric value, different each time", () => {
    const { f, el, q, btn } = setup();
    const gen = el.querySelector<HTMLButtonElement>("button[data-generate]")!;
    expect(gen.type).toBe("button");
    gen.click();
    f.detectChanges();
    const first = q("value").value;
    expect(first).toMatch(/^[A-Za-z0-9]{32}$/);
    expect(btn().disabled).toBe(true); // name still required
    gen.click();
    f.detectChanges();
    expect(q("value").value).toMatch(/^[A-Za-z0-9]{32}$/);
    expect(q("value").value).not.toBe(first);
  });

  it("rejects names outside the charset with inline help", () => {
    const { type, btn, el } = setup();
    type("value", "v");
    type("name", "bad name");
    expect(btn().disabled).toBe(true);
    expect(el.textContent).toContain("letters, digits");
  });

  it("sends value null when the value is untouched", async () => {
    const { save, saveEntry } = setup(META);
    await save();
    expect(saveEntry).toHaveBeenCalledWith({ project: "p", environment: "e", name: "OLD", renameFrom: null, value: null, notes: "n" });
  });

  it("sends renameFrom and warns about references", async () => {
    const { type, save, saveEntry, el } = setup(META);
    type("name", "NEW");
    expect(el.textContent).toContain("stop resolving");
    await save();
    expect(saveEntry.mock.calls[0][0]).toMatchObject({ name: "NEW", renameFrom: "OLD" });
  });

  it("shows the server's InvalidInput", async () => {
    const { type, save, el } = setup(null, vi.fn().mockRejectedValue({ InvalidInput: "already exists" }));
    type("name", "K");
    type("value", "v");
    await save();
    expect(el.textContent).toContain("already exists");
  });

  it("uses a masked multi-line value field that skips autofill and spellcheck", () => {
    const { q } = setup();
    const i = q("value");
    expect(i.tagName).toBe("TEXTAREA");
    expect(["autocomplete", "spellcheck", "autocapitalize", "autocorrect"].map((a) => i.getAttribute(a))).toEqual(["off", "false", "off", "off"]);
    expect(i.classList).toContain("secret");
  });

  it("blocks copy and cut of the masked value", () => {
    const { q } = setup();
    for (const type of ["copy", "cut"]) {
      const ev = new Event(type, { cancelable: true, bubbles: true });
      q("value").dispatchEvent(ev);
      expect(ev.defaultPrevented).toBe(true);
    }
  });

  it("keeps inner line breaks", async () => {
    const { type, save, saveEntry } = setup();
    type("name", "K");
    type("value", "line1\nline2");
    await save();
    expect(saveEntry.mock.calls[0][0].value).toBe("line1\nline2");
  });

  it("strips trailing line breaks on save", async () => {
    const { type, save, saveEntry } = setup();
    type("name", "K");
    type("value", "sk-abc\r\n\n");
    await save();
    expect(saveEntry.mock.calls[0][0].value).toBe("sk-abc");
  });

  it("does not accept a value of only line breaks for a new entry", () => {
    const { type, btn } = setup();
    type("name", "K");
    type("value", "\n\n");
    expect(btn().disabled).toBe(true);
  });
});
