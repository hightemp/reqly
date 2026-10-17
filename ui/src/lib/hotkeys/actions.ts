import type { ActionDefinition } from './hotkeys';

/** Default shortcuts (PRD §7.19). Users can override them in settings. */
export const DEFAULT_ACTIONS: ActionDefinition[] = [
  { id: 'request.send', combos: ['Mod+Enter'], scope: 'global' },
  { id: 'request.save', combos: ['Mod+S'], scope: 'global' },
  { id: 'request.new', combos: ['Mod+N'], scope: 'global' },
  { id: 'folder.new', combos: ['Mod+Shift+N'], scope: 'global' },
  { id: 'tab.close', combos: ['Mod+W'], scope: 'global' },
  { id: 'tab.reopen', combos: ['Mod+Shift+T'], scope: 'global' },
  { id: 'tab.next', combos: ['Ctrl+Tab'], scope: 'global' },
  { id: 'tab.prev', combos: ['Ctrl+Shift+Tab'], scope: 'global' },
  { id: 'palette.open', combos: ['Mod+K', 'Mod+P'], scope: 'global' },
  { id: 'url.focus', combos: ['Mod+L'], scope: 'global' },
  { id: 'sidebar.toggle', combos: ['Mod+B'], scope: 'global' },
  { id: 'layout.toggle', combos: ['Mod+Alt+L'], scope: 'global' },
  { id: 'tree.search', combos: ['Mod+Shift+F'], scope: 'global' },
  { id: 'body.format', combos: ['Mod+Shift+B'], scope: 'global' },
  { id: 'environments.open', combos: ['Mod+E'], scope: 'global' },
  { id: 'settings.open', combos: ['Mod+,'], scope: 'global' },
  { id: 'tree.rename', combos: ['F2'], scope: 'tree' },
  { id: 'tree.delete', combos: ['Del'], scope: 'tree' },
  { id: 'tree.duplicate', combos: ['Mod+D'], scope: 'tree' },
  { id: 'tree.cut', combos: ['Mod+X'], scope: 'tree' },
  { id: 'tree.copy', combos: ['Mod+C'], scope: 'tree' },
  { id: 'tree.paste', combos: ['Mod+V'], scope: 'tree' },
  { id: 'tree.selectAll', combos: ['Mod+A'], scope: 'tree' },
];
