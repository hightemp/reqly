# Sync

The desktop and Android apps can keep workspaces in sync with a Reqly
server. Work offline as usual; changes go to the server when it is
reachable, and others on the server see them.

## Connecting

**Settings → Sync**: enter the server URL and your user name and password
(or an API token). The password is not stored: Reqly keeps a personal API
token instead, which you can revoke in the web UI.

## Choosing workspaces

The list shows the workspaces of this device and of the server. Tick one to
sync it: a workspace only on this device is uploaded (and belongs to you on
the server), one only on the server is downloaded. Unticking stops syncing;
nothing is deleted.

## When it syncs

At connect, every 30 seconds, shortly after a change and on **Sync now**
(settings, command palette). The cloud in the status bar shows the state:
synced, changes waiting, syncing, failed or conflicts.

## What syncs

Workspaces, collections, folders, requests and environments. History,
cookies, OAuth 2.0 tokens and app settings stay on each device. Secret
values stay on the device unless **Sync secret values** is on.

## Conflicts

Changes merge field by field, and text fields merge edit by edit. When
something was deleted on one side and changed on the other, the cloud shows
a conflict: open it and choose which version to keep.
