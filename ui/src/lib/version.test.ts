import { describe, expect, it } from 'vitest';
import pkg from '../../package.json';
import { APP_VERSION, appTitle } from './version';

describe('app version', () => {
  it('comes from package.json (set by `make release`)', () => {
    expect(APP_VERSION).toBe(pkg.version);
    expect(APP_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });

  it('goes into the window title', () => {
    expect(appTitle()).toBe(`Reqly ${pkg.version}`);
  });
});
