import type { Services } from '$lib/services.svelte';
import type { Tab } from '$lib/stores/tabs.svelte';
import { protocolOf } from '$lib/ui/badges';

/** What a tab shows: names come from drafts so tabs of other collections still show them. */
export function tabInfo(services: Services, tab: Tab) {
  const draft = services.editor.get(tab.id);
  const item = services.tree.byId.get(tab.id);
  return {
    name: draft?.node?.name ?? item?.label ?? '…',
    method: draft?.request?.method ?? item?.method ?? 'GET',
    protocol: draft?.node ? protocolOf(draft.node.data) : (item?.protocol ?? null),
    dirty: draft?.dirty ?? false,
    running: services.responses.run(tab.id).sending,
  };
}

/** Closes a tab and lets go of its draft. */
export function closeTab(services: Services, id: string) {
  void services.editor.release(id);
  services.tabs.close(id);
}

/**
 * Loads the draft of every open tab (for its title) and closes tabs whose
 * request is gone. Call inside a component.
 */
export function trackTabs(services: Services) {
  $effect(() => {
    for (const tab of services.tabs.tabs) {
      void services.editor.open(tab.id).then(
        (d) => {
          if (d.node?.deletedAt) services.tabs.close(tab.id);
        },
        () => services.tabs.close(tab.id),
      );
    }
  });
}
