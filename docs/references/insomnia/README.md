# References: Insomnia 10.2

Captured on 2026-10-17 from an isolated profile (Scratch Pad, a demo
collection at `127.0.0.1:8484`, no personal data). For each screen: what we
take and what we do differently.

| File | Screen | We take | We change |
|---|---|---|---|
| `01-tree.png` | Collection tree | Compact rows, method badge left of the name, nesting indents, filter on top, sort and "collapse all" next to the filter | Method badges use `--c-method-*` tokens (Insomnia's GET is purple, ours green); we add the "categories + list" view and multi-select |
| `02-missing-variable.png` | Missing variable | Variable "pill" in the URL, red with ⚠ when not found | Pills with a value preview on hover (stage 2) |
| `03-env-switcher-and-headers.png` | Environment switcher and Headers tab | Automatic headers (`Accept`, `Host <calculated at runtime>`) shown separately above user headers; count on the tab | We also show headers inherited from folders with their source |
| `04-env-editor-table.png`, `05-env-editor-raw.png` | Environment editor | Table / raw JSON switch, environment list on the left | Environment colors, secret flag, sub-environments (stage 2) |
| `06-request-response.png` | Request and response | Three-column layout, status badge, time and size in the response header, JSONPath filter at the bottom of the response, "Beautify JSON" below the body | We add tabs for open requests (like Postman) and a "response below" layout |
| `07-response-headers.png` | Response headers | Table and count | — |
| `08-context-menu-folder.png`, `09-context-menu-request.png` | Context menus | Items grouped in sections (Create / Import / Actions), shortcuts on the right | We add Cut, Copy and Paste, bulk actions and "Copy as cURL" |
| `10-themes.png` | Theme picker | Grid of theme previews with a mini layout and palette; "Use OS color scheme" flag | Our own theme set, a theme editor and JSON theme import |
| `11-import-preview.png` | Import | Preview of what will be imported with counts | A detailed report of unsupported data (stage 3) |

What gets in the way in Insomnia and we will not have: the permanent sign-in
banner, dropdowns built on native `<select>` ("No Global Environment"), and
response tabs that do not fit the width (the Timeline tab slides out of view
without a scroll indicator).
