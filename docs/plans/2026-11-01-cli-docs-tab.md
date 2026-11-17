# CLI Documentation Tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a fifth "Help" sidebar tab with a question-mark icon, change the "Settings" tab icon to a gear, and display inline CLI usage instructions with an example.

**Architecture:** Extend `AppComponent.tabs` with the updated gear icon for Settings and a fifth `help` tab entry with a question-mark icon. Render the static CLI documentation and example inside `@case ("help")` in `app.component.html`, styled in `app.component.css`.

**Tech Stack:** Angular 22 (standalone components, signals), TypeScript 6, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-01-cli-docs-tab-design.md`

## Global Constraints

- No new backend IPC commands or state.
- No extra standalone component files.
- Don't hand-edit `package-lock.json`, `Cargo.lock` or `src-tauri/gen/`.
- Preserve keyboard navigation (`ArrowRight`, `ArrowLeft`, `Home`, `End`) and roving `tabindex` across all five tabs in `AppComponent`.

## Review Focus

1. Keyboard navigation and roving `tabindex` in `AppComponent` when a 5th tab is added: `ArrowRight` wraps across 5 tabs (`this.tabs.length`) and keeps a single `tabIndex === 0`. Tested in `src/app/app.component.spec.ts`.

---

### Task 1: Add Help tab, update Settings gear icon, and render CLI documentation

**Files:**
- Modify: `src/app/app.component.ts:36-41`
- Modify: `src/app/app.component.html:42-48`
- Modify: `src/app/app.component.css:1-23`
- Test: `src/app/app.component.spec.ts:62-78`

**Interfaces:**
- Consumes: `AppComponent.tabs` (`readonly [id, label, svgPath][]`), `AppComponent.tab` signal.
- Produces: 5th tab `"help"` ("Help") in `AppComponent.tabs` and `@case ("help")` panel in `app.component.html`.

- [ ] **Step 1: Write the failing tests in `src/app/app.component.spec.ts`**

Update the existing tab list and arrow-key tests for 5 tabs, and add a test that clicking the Help tab renders the CLI instructions and example:

```ts
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx ng test --watch=false`
Expected: FAIL on `AppComponent` tab expectations (`expected [ 'Vault', 'Approvals', 'Audit', 'Settings' ] to deeply equal [ 'Vault', 'Approvals', 'Audit', 'Settings', 'Help' ]`).

- [ ] **Step 3: Implement the tab icons, Help tab markup, and styles**

In `src/app/app.component.ts`, replace the `settings` icon path with a gear icon and append the `help` tab with a question-mark icon:

```ts
  readonly tabs = [
    ["vault", "Vault", "M4 4h16v15H4z M7 19v2 M17 19v2 M8.5 11.5a3.5 3.5 0 1 0 7 0 3.5 3.5 0 1 0-7 0 M12 5.5v2 M12 15.5v2 M6.5 11.5h2 M15.5 11.5h2"],
    ["approvals", "Approvals", "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z M9 12l2 2 4-4"],
    ["audit", "Audit", "M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01"],
    ["settings", "Settings", "M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6 M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"],
    ["help", "Help", "M12 3a9 9 0 1 0 0 18 9 9 0 1 0 0-18 M9.5 9.5a2.5 2.5 0 0 1 4.6 1.3c0 1.5-2.1 2-2.1 3.2 M12 17h.01"],
  ] as const;
```

In `src/app/app.component.html`, add `@case ("help")` inside `@switch (tab())`:

```html
          @case ("help") {
            <section class="help">
              <h2>Using the CLI</h2>
              <p>Keep secrets out of <code>.env</code> files by replacing values with <code>op://&lt;project&gt;/&lt;environment&gt;/&lt;entry&gt;</code> references and running your command through <code>op run</code>.</p>
              <h3>1. Reference secrets in <code>.env</code></h3>
              <p class="muted">Use Copy Reference in Vault to copy an entry's reference.</p>
              <pre><code>DATABASE_URL=op://myapp/dev/DATABASE_URL
LOG_LEVEL=debug</code></pre>
              <h3>2. Run your command</h3>
              <pre><code>op run -- npm start
op run --env-file .env.local -- node server.js</code></pre>
              <h3>3. Approve in the app</h3>
              <p>One-pass must be running. When <code>op run</code> connects, approve the prompt to start the command with the resolved secrets in its environment.</p>
            </section>
          }
```

In `src/app/app.component.css`, style `.help` and its `<pre>` blocks:

```css
.help { padding: 1rem; display: grid; gap: 0.75rem; align-content: start; max-width: 36rem; }
.help h2, .help h3, .help p, .help pre { margin: 0; }
.help h2 { font-size: 1rem; }
.help h3 { font-size: 0.9rem; margin-top: 0.25rem; }
.help pre { padding: 0.6rem 0.75rem; background: var(--surface); border: 1px solid var(--line); border-radius: 6px; overflow-x: auto; }
```

- [ ] **Step 4: Run tests and production build to verify they pass**

Run: `npx ng test --watch=false && npm run build`
Expected: PASS with all spec suites green and clean production build.

