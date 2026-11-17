# Vault usability

Date: 2026-11-16. Four small additions to the vault browser. No new backend commands.

## Language

- **Entry search**: a query that filters entries across every project and environment. It matches the entry name, notes, project or environment, ignoring case. It never looks at values (the frontend does not have them).
- **Generated value**: a random value the entry editor writes into the value field.
- **Multi-line value**: an entry value containing line breaks, such as a PEM key.

## Rules

- Search: with an empty query the list shows the selected environment as today. With a query it shows every match, each labelled with its project and environment. Choosing a match selects its environment and the entry. Escape in the search field clears it.
- Generated value: 32 characters from `A-Z a-z 0-9`, drawn with `crypto.getRandomValues` and rejection sampling (no modulo bias). It replaces whatever is in the value field. Generated in the frontend because the value field already lives there; nothing extra crosses IPC.
- Multi-line value: the value field is a masked multi-line field. Trailing line breaks are removed on save, so a pasted single-line key with a stray newline is stored clean. Inner line breaks are kept. Enter in the value field adds a line; saving is by the Save button.
- Reveal shows line breaks as line breaks.
- Shortcuts while unlocked: Ctrl/Cmd+F opens the Vault tab and focuses search. Ctrl/Cmd+L locks. Both are listed in Help.

## Not included

Generator options (length, symbols), search inside values, fuzzy matching.
