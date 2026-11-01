# CLI documentation tab

## Intent
Give users an in-app quick reference and example for using the `op` CLI without leaving the app.

## Goals
- Add a fifth sidebar tab (**Help**) with a question-mark icon.
- Change the **Settings** tab icon from sliders to a gear.
- Show instructions and a concrete example for referencing secrets in `.env` and running commands with `op run`.

## Non-goals & Anti-goals
- No new backend IPC commands or state.
- No extra standalone component files for static markup.

## Approaches considered
- **A. Inline `<section>` in `app.component.html` (chosen):** smallest change; static HTML needs no component state or IPC.
- B. Standalone component in `src/app/screens/`: unnecessary boilerplate for static content.
- C. Section inside Settings: less discoverable; user requested a fifth sidebar tab.

## Design
1. **Tabs (`src/app/app.component.ts`):**
   - Replace the `settings` tab SVG path with a gear icon.
   - Append a fifth tab `["help", "Help", "..."]` with a question-mark-in-circle SVG path.
2. **Content (`src/app/app.component.html`, `src/app/app.component.css`):**
   - Add `@case ("help")` inside `.panel` rendering a `<section class="help">` with:
     - How to copy an `op://<project>/<environment>/<entry>` reference from **Vault** into `.env` (with an example `.env` block).
     - How to run a command with `op run -- <command>` and `--env-file <path>` (with example commands).
     - A brief note that the app must be running to approve the request and inject the secrets into the child process environment.
3. **Tests (`src/app/app.component.spec.ts`):**
   - Update tab list assertions for the 5th tab and test switching to the Help tab to verify the CLI instructions and example render.

## Left to the builder
- Exact SVG path coordinates and `<pre>` styling matching `src/styles.css`.

