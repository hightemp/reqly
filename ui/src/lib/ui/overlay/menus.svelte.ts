// Menu model and the global context-menu service.

import type { Rect } from './position';

export interface MenuItem {
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  checked?: boolean;
  children?: MenuEntry[];
  onSelect?: () => void;
}

export type MenuEntry = MenuItem | { separator: true } | { heading: string };

export const isItem = (e: MenuEntry): e is MenuItem => 'id' in e;

interface OpenMenu {
  anchor: HTMLElement | Rect;
  items: MenuEntry[];
  /** Focus returns here when the menu closes. */
  returnFocus: HTMLElement | null;
}

class MenuService {
  current = $state<OpenMenu | null>(null);

  /** Opens a context menu at a point (e.g. from a `contextmenu` event). */
  openAt(point: { x: number; y: number }, items: MenuEntry[]) {
    this.current = {
      anchor: { x: point.x, y: point.y, width: 0, height: 0 },
      items,
      returnFocus: document.activeElement as HTMLElement | null,
    };
  }

  /** Opens a menu below an element (dropdown buttons). */
  openBelow(anchor: HTMLElement, items: MenuEntry[]) {
    this.current = { anchor, items, returnFocus: anchor };
  }

  close(restoreFocus = true) {
    const focus = this.current?.returnFocus;
    this.current = null;
    if (restoreFocus) focus?.focus?.();
  }
}

export const menus = new MenuService();

/** Opens `items` as a context menu for a `contextmenu` event. */
export function contextMenu(e: MouseEvent, items: MenuEntry[]) {
  e.preventDefault();
  e.stopPropagation();
  menus.openAt({ x: e.clientX, y: e.clientY }, items);
}
