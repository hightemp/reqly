// Source of truth for message keys. Every other locale must match this shape.
export const en = {
  app: {
    name: 'Reqly',
    loading: 'Loading…',
    connectionLost: 'Connection to the server lost. Reconnecting…',
  },
  common: {
    ok: 'OK',
    cancel: 'Cancel',
    close: 'Close',
  },
  kv: {
    name: 'Name',
    value: 'Value',
    newName: 'New name',
    newValue: 'New value',
    description: 'Description',
    enabled: 'Enabled',
    deleteRow: 'Delete row',
    deleteAll: 'Delete all',
    dragHandle: 'Drag to reorder',
    bulkEdit: 'Bulk edit',
    tableEdit: 'Table edit',
    showDescription: 'Description',
    hideDescription: 'Hide description',
    kind: 'Field type',
    kindText: 'Text',
    kindFile: 'File',
    chooseFile: 'Choose file…',
    inheritedFrom: 'Inherited from {source}',
  },
  tree: {
    delete: 'Delete',
    deleteOneTitle: 'Delete “{name}”?',
    deleteManyTitle: 'Delete {count} items?',
    deleteMessage: 'Items go to the trash and can be restored.',
    deleted: { one: 'Deleted {count} item', other: 'Deleted {count} items' },
    undo: 'Undo',
    curlCopied: { one: 'Copied {count} cURL command', other: 'Copied {count} cURL commands' },
  },
  window: {
    minimize: 'Minimize',
    maximize: 'Maximize',
    restore: 'Restore',
    close: 'Close',
  },
  sidebar: {
    title: 'Collections',
    empty: 'No requests yet',
    collapse: 'Hide sidebar',
    expand: 'Show sidebar',
  },
  request: {
    empty: 'Select or create a request',
  },
  response: {
    empty: 'Send a request to see the response',
  },
  settings: {
    theme: 'Theme',
    themeSystem: 'System',
    language: 'Language',
    layout: 'Toggle response layout',
  },
  languages: {
    en: 'English',
    ru: 'Русский',
  },
  errors: {
    unknown: 'Something went wrong',
    'transport.network': 'Cannot reach the Reqly server',
    'command.unknown': 'Unknown command "{name}"',
  },
} as const;

export type Messages = typeof en;
