# Working together

Reqly can run as a server in the browser for a team. Everyone signs in with
their own account; administrators manage users in **Settings →
Administration**.

## Sharing a workspace

**Workspace actions → Members…** invites people by an invitation link and
sets their role:

| Role   | Can                                                              |
| ------ | ---------------------------------------------------------------- |
| Viewer | open and send requests, run scripts, export, switch environments |
| Editor | also change requests, folders, collections, environments, import |
| Owner  | also rename or delete the workspace and manage members           |

Viewers can try changes and send requests, but nothing they change is saved.

## Editing at the same time

- Changes of others appear at once, without reloading.
- When two people edit the same request, changes to different fields both
  survive; for the same field the last save wins and the other person is
  told who changed it.
- Bodies, GraphQL queries and scripts are edited together in real time, with
  the others' cursors and names.
- Avatars show who is in the workspace and who has a request open.

## Response history

Each person sees their own responses. An owner can turn on **Share response
history** in **Members…** so that everyone sees all responses.

## API tokens

**Settings → Account** creates personal API tokens for scripts, the CLI and
[desktop sync](help:sync).
