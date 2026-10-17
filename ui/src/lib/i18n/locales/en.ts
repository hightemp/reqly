// Source of truth for message keys. Every other locale must match this shape.
export const en = {
  app: {
    name: 'Reqly',
    loading: 'Loading…',
    connectionLost: 'Connection to the server lost. Reconnecting…',
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
