# Multi-user server

`reqly-server` has three authentication modes (`auth.mode`,
`REQLY_AUTH_MODE`):

| Mode | Who signs in | Use it for |
|---|---|---|
| `none` | nobody | localhost or a trusted network |
| `token` | anyone with the shared token | one person |
| `multi` | users with their own accounts | a team |

## Starting a team server

```sh
REQLY_AUTH_MODE=multi \
REQLY_ADMIN_USERNAME=admin \
REQLY_ADMIN_PASSWORD='a long password' \
REQLY_SECURE_COOKIE=true \
reqly-server
```

The first administrator is created from `REQLY_ADMIN_USERNAME` and
`REQLY_ADMIN_PASSWORD` when the database has no users yet; later starts
ignore them. You can also create administrators from the command line, and
reset a lost password the same way:

```sh
echo 'a long password' | reqly-server admin create alice
echo 'a new password'  | reqly-server admin password alice
```

The password comes from `REQLY_ADMIN_PASSWORD` or the first line of stdin.
Run the commands with the same `REQLY_DATA_DIR` (or `reqly.toml`) as the
server.

## Accounts

`auth.registration` (`REQLY_REGISTRATION`) decides who creates accounts:

- `admin` (default): only administrators, in the admin panel or with
  `reqly-server admin create`;
- `open`: anyone who can reach the server, from the sign-in screen;
- `invite`: only with an invitation link.

User names are 2–64 letters, digits, `.`, `_`, `-` or `@` and are not
case-sensitive. Passwords have at least 8 characters and are stored as
argon2id hashes. After 5 failed sign-ins an account is locked for
15 minutes; each address is also limited to 10 attempts a minute.

**Settings → Account** changes the display name and the password (which ends
every session of the user) and manages **API tokens** for the CLI and
scripts:

```sh
curl -X POST https://reqly.example.com/api/v1/cmd/workspace.list \
  -H 'Authorization: Bearer rq_…' -H 'content-type: application/json' -d '{}'
```

A token is shown once when created; the server keeps only its SHA-256.
Tokens can expire and can be deleted at any time.

## Invitation links

**Workspace actions → Members… → Create link** makes a link that adds
whoever opens it to the workspace with the chosen role. A new person
creates an account on the way (also when `registration` is `invite`); a
signed-in user just joins. A link works once and expires after 7 days;
open links are listed in the same dialog and can be revoked. The server
stores only a hash of the link's token. An invitation never lowers a role
someone already has.

Administrators can also create links that only create an account
(`admin.invite`), for servers where registration is by invitation.

## Workspaces and roles

Everyone gets a workspace of their own on first sign-in. **Workspace
actions → Members…** shares a workspace with other users:

| Role | Can |
|---|---|
| Viewer | open requests, send them, run scripts, see history, export, switch environments |
| Editor | also change requests, folders, collections, environments and cookies, import |
| Owner | also rename and delete the workspace and manage its members |

A workspace always keeps at least one owner. Administrators act as owners
of every workspace. Viewers see the workspace in view-only mode: they can
try changes and send requests, but nothing is saved.

Each user has their own settings (theme, shortcuts…) and their own active
environment; a setting a user never changed falls back to the server-wide
value. WebSocket, SSE and gRPC connections belong to the user who opened
them.

The server checks every command against the caller's role in the
workspaces it touches (ADR-010), so the rules hold for the API as well as
for the UI.
