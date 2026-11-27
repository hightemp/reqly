# Settings

Open **Settings** with `Ctrl+,` or the gear in the status bar.

| Section            |                                                        |
| ------------------ | ------------------------------------------------------ |
| General            | language, autosave, sidebar view, response position    |
| Appearance         | theme, density, fonts, interface scale, window frame   |
| Custom themes      | create, edit, import and export themes (JSON)          |
| Requests & proxy   | timeouts, redirects, TLS, HTTP version, proxy          |
| Certificates       | CA certificates and client certificates (PEM, PKCS#12) |
| Templates          | the fake data locale                                   |
| History            | how long and how many responses to keep                |
| Keyboard shortcuts | change any [shortcut](help:shortcuts)                  |
| Sync               | [desktop sync](help:sync) with a server                |
| MCP                | the [MCP server](help:mcp) for AI agents               |
| AI                 | the [AI chat](help:ai): providers, model, tools        |
| About & data       | version, data folder, cleanup, updates                 |

The status bar switches the theme, the language and the response position
(right or below) with one click.

## Certificate check

Requests check the server certificate. For a test server with a self-signed
certificate, turn **Verify TLS certificates** off in Settings → Requests &
proxy (all requests) or on the **Settings** tab of one request, or add the
server's CA in **Certificates**. When a response fails with a certificate
error, **Send without checking the certificate** turns the check off for that
request and sends it again.

## Updates

Reqly checks for a new version at start and once a day. **Check for updates**
(Settings → About & data, command palette) asks right away. The desktop app
downloads the update, checks its signature and installs it; **Restart now**
finishes. Turn on **Install updates automatically** to do this in the
background, or choose the **Pre-release** channel to get beta versions.

## Data

The desktop app keeps everything in a local database; secret values are
encrypted with a key stored next to it. Copy both when moving to another
computer, or use [Export](help:import-export) or [Sync](help:sync).
