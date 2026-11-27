// The app version: `make release` writes it into package.json together with
// Cargo.toml, so the UI and the core agree.

import { version } from '../../package.json';

export const APP_VERSION: string = version;

/** The window (and browser tab) title. */
export function appTitle(): string {
  return `Reqly ${APP_VERSION}`;
}
