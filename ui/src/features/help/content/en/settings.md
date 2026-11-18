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
| About & data       | version, data folder, cleanup, updates                 |

The status bar switches the theme, the language and the response position
(right or below) with one click.

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
