# Import and export

## Import

**Workspace actions → Import…**, **Import into this collection…** in the
collection menu, **Import into this folder…** on a folder, or the command
palette. The wizard takes files, a folder, dropped files, pasted text or a
URL; the format is detected automatically. **Supported formats** in the
wizard lists them with examples.

Before importing you see the tree, the number of requests and environments
and a **report** of everything that could not be imported exactly. Choose
where to import: a new collection, or an existing collection or folder —
_merge_ (same names are updated), _append_ or _replace_ (old items go to the
trash).

| Format                      |                                                      |
| --------------------------- | ---------------------------------------------------- |
| Reqly (JSON / YAML)         | everything, including environments and history       |
| Insomnia v3–v5              | workspaces, folders, requests, environments, cookies |
| Postman v2.0 / v2.1         | collections, environments, globals, scripts          |
| OpenAPI 3.x, Swagger 2.0    | tags → folders, servers → `base_url`                 |
| HAR 1.2                     | recorded requests from browser DevTools              |
| cURL                        | one or many commands                                 |
| `.http` / `.rest`, raw HTTP | JetBrains HTTP Client, VS Code REST Client           |
| Bruno                       | a collection folder or zip                           |

## Requests from the clipboard

- Paste a `curl` command or an HTTP request into the URL bar to replace the
  current request.
- **Import from clipboard** in the context menu of a folder or request adds
  every curl command or HTTP request on the clipboard to that folder.
- **Import requests from cURL or HTTP text…** (command palette) opens the
  wizard on the paste tab.

An HTTP request looks like this; separate several with `###`:

```
### Create an item
POST https://api.example.com/items
Content-Type: application/json

{"name": "Ada"}
```

## Export

**Export workspace…**, **Export collection…** or **Export…** in the context
menu of selected items. Formats: Reqly (JSON or YAML), Insomnia v4 and v5,
Postman v2.1, HAR and cURL. Secret values are left empty unless you include
them.

File names get the date and time, e.g. `my-api-2026-11-27_14-05.reqly.json`,
so exports do not overwrite each other. Turn it off in the export window or in
**Settings → General**.
