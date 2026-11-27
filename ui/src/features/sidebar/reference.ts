// "Copy path / ID / for AI" for requests and folders: text that points to a
// node, e.g. for an agent that reads it through the MCP tool `get_request`.

import type { Node } from '$lib/api/generated/Node';
import type { Transport } from '$lib/api/transport';
import { errorMessage } from '$lib/errors';
import { t } from '$lib/i18n';
import type { Services } from '$lib/services.svelte';
import type { MenuEntry } from '$lib/ui/overlay/menus.svelte';
import { toasts } from '$lib/ui/overlay/toasts.svelte';

export interface NodeReference {
  id: string;
  name: string;
  folder: boolean;
  /** Collection, folders and the node itself. */
  path: string[];
  method?: string;
  url?: string;
}

type Call = Transport['call'];

/** Loads `id` and its parents; `collections` give the collection's name. */
export async function loadReference(
  call: Call,
  collections: { id: string; name: string }[],
  id: string,
): Promise<NodeReference> {
  const node = (await call('node.get', { id })) as Node;
  const path = [node.name];
  // Parents up to the collection root (bounded in case of a broken chain).
  let parent = node.parentId;
  for (let i = 0; parent && i < 64; i++) {
    const p = (await call('node.get', { id: parent })) as Node;
    path.unshift(p.name);
    parent = p.parentId;
  }
  const collection = collections.find((c) => c.id === node.collectionId);
  if (collection) path.unshift(collection.name);
  const data = node.data as { kind: string; method?: string; url?: string };
  const ref: NodeReference = { id: node.id, name: node.name, folder: data.kind === 'folder', path };
  if (data.kind === 'request') ref.method = data.method;
  else if (data.kind === 'webSocket') ref.method = 'WS';
  else if (data.kind === 'grpc') ref.method = 'gRPC';
  if (!ref.folder) ref.url = data.url ?? '';
  return ref;
}

export function pathText(ref: NodeReference): string {
  return ref.path.join(' / ');
}

/** A block to paste into a chat with an agent. */
export function aiText(ref: NodeReference): string {
  const lines = [
    `Reqly ${ref.folder ? 'folder' : 'request'} "${ref.name}"`,
    `Path: ${pathText(ref)}`,
    `ID: ${ref.id}`,
  ];
  if (!ref.folder) lines.push(`${ref.method ?? ''} ${ref.url ?? ''}`.trim());
  lines.push(`Read it with the Reqly MCP tool get_request {"id": "${ref.id}"}.`);
  return lines.join('\n');
}

/** The "Copy path / ID" submenu for a request or folder. */
export function referenceMenu(s: Services, id: string): MenuEntry {
  const copy = async (text: () => Promise<string> | string, message: string) => {
    try {
      await navigator.clipboard.writeText(await text());
      toasts.show({ message, kind: 'success' });
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  };
  const load = async () => {
    // An open editor may hold changes not saved yet (the URL, the name).
    await s.editor.flush(id).catch(() => {});
    return loadReference(s.transport.call.bind(s.transport), s.workspace.collections, id);
  };
  return {
    id: 'copy-ref',
    label: t('actions.copyReference'),
    children: [
      {
        id: 'copy-path',
        label: t('actions.copyPath'),
        onSelect: () => void copy(async () => pathText(await load()), t('actions.pathCopied')),
      },
      {
        id: 'copy-id',
        label: t('actions.copyId'),
        onSelect: () => void copy(() => id, t('actions.idCopied')),
      },
      {
        id: 'copy-ai',
        label: t('actions.copyForAi'),
        onSelect: () => void copy(async () => aiText(await load()), t('actions.aiCopied')),
      },
    ],
  };
}
