import { TestBed } from "@angular/core/testing";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PromptView } from "../../api/types";
import { VaultApi } from "../../api/vault-api";
import { ApprovalDialogComponent } from "./approval-dialog.component";

const view = (id: number, over: Partial<PromptView> = {}): PromptView => ({
  id,
  projectDir: "/work/proj",
  command: ["node", "app.js"],
  commandName: "node",
  peerPid: 4242,
  peerExe: "/usr/bin/op",
  variables: [["API_KEY", "op://p/e/API_KEY"]],
  references: ["op://p/e/API_KEY"],
  mode: "time_boxed",
  ...over,
});

async function setup(initial: PromptView[], respondApproval = vi.fn().mockResolvedValue(undefined)) {
  let list = initial;
  let changed: () => void = () => {};
  const fake = {
    listPendingApprovals: vi.fn(async () => list),
    respondApproval,
    onApprovalsChanged: (cb: () => void) => ((changed = cb), () => {}),
  };
  TestBed.configureTestingModule({ providers: [{ provide: VaultApi, useValue: fake }] });
  const f = TestBed.createComponent(ApprovalDialogComponent);
  const el: HTMLElement = f.nativeElement;
  const flush = async (ms = 0) => {
    await vi.advanceTimersByTimeAsync(ms);
    f.detectChanges();
    await vi.advanceTimersByTimeAsync(0);
    f.detectChanges();
  };
  f.detectChanges();
  await flush();
  return {
    el,
    fake,
    flush,
    set: async (l: PromptView[]) => ((list = l), changed(), flush()),
    q: (t: string) => el.querySelector<HTMLElement>(`[data-testid=${t}]`) as HTMLButtonElement,
  };
}

describe("ApprovalDialogComponent", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("shows a prompt that arrived before the listener", async () => {
    const { el } = await setup([view(1)]);
    expect(el.textContent).toContain("/work/proj");
  });

  it("refetches on approvals-changed", async () => {
    const { el, set } = await setup([]);
    expect(el.querySelector(".dialog")).toBeNull();
    await set([view(1)]);
    expect(el.querySelector(".dialog")).toBeTruthy();
  });

  it("renders caller details and treats markup as text", async () => {
    const { el, q } = await setup([view(1, { command: ["<b>x</b>"] })]);
    expect(q("command").textContent).toBe("<b>x</b>");
    expect(el.querySelector("b")).toBeNull();
    expect(el.textContent).toContain("PID 4242");
    expect(el.textContent).toContain("/usr/bin/op");
    expect(el.textContent).toContain("API_KEY");
    expect(el.textContent).toContain("op://p/e/API_KEY");
  });

  it("preselects the project's mode and shows duration only for time-boxed", async () => {
    const { el, flush } = await setup([view(1)]);
    expect(el.querySelector<HTMLSelectElement>("[name=duration]")!.value).toBe("min15");
    const mode = el.querySelector<HTMLSelectElement>("[name=mode]")!;
    mode.value = "remember";
    mode.dispatchEvent(new Event("change"));
    await flush();
    expect(el.querySelector("[name=duration]")).toBeNull();
  });

  it("preselects per_run from the view", async () => {
    const { el } = await setup([view(1, { mode: "per_run" })]);
    expect(el.querySelector<HTMLSelectElement>("[name=mode]")!.value).toBe("per_run");
  });

  it("disables Allow for 500 ms and focuses Deny", async () => {
    const { q, flush, fake } = await setup([view(1)]);
    expect(q("allow").disabled).toBe(true);
    expect(document.activeElement).toBe(q("deny"));
    q("allow").click();
    expect(fake.respondApproval).not.toHaveBeenCalled();
    await flush(500);
    expect(q("allow").disabled).toBe(false);
  });

  it("sends the chosen decision", async () => {
    const { q, flush, fake } = await setup([view(1)]);
    await flush(500);
    q("allow").click();
    await flush();
    expect(fake.respondApproval).toHaveBeenCalledWith(1, { allow: { mode: "time_boxed", duration: "min15" } });
  });

  it("shows one prompt at a time and re-arms for the next", async () => {
    const respond = vi.fn().mockResolvedValue(undefined);
    let list = [view(1), view(2, { projectDir: "/other" })];
    const { el, q, flush, fake } = await setup(list, respond);
    expect(el.textContent).toContain("1 more waiting");
    expect(el.textContent).not.toContain("/other");
    fake.listPendingApprovals.mockImplementation(async () => (list = list.slice(1)));
    await flush(500);
    q("deny").click();
    await flush();
    expect(el.textContent).toContain("/other");
    expect(q("allow").disabled).toBe(true);
  });

  it("closes when the prompt is removed server-side", async () => {
    const { el, set } = await setup([view(1)]);
    await set([]);
    expect(el.querySelector(".dialog")).toBeNull();
  });

  it("closes quietly when respond fails with an unknown id", async () => {
    const { el, q, flush, fake } = await setup([view(1)], vi.fn().mockRejectedValue({ InvalidInput: "no such approval" }));
    fake.listPendingApprovals.mockResolvedValue([]);
    q("deny").click();
    await flush();
    expect(el.querySelector(".dialog")).toBeNull();
    expect(el.textContent).not.toContain("no such approval");
  });
});
