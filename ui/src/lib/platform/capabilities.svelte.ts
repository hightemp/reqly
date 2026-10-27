// What the platform hosting the core can do (`app.info`): the Android app
// has no Unix sockets, interface binding or `os` tag.

import type { Platform } from '$lib/api/generated/Platform';
import type { Transport } from '$lib/api/transport';

export class Capabilities {
  platform = $state<Platform | null>(null);
  unavailable = $state<string[]>([]);

  get mobile(): boolean {
    return this.platform === 'mobile';
  }

  has(feature: string): boolean {
    return !this.unavailable.includes(feature);
  }

  set(platform: Platform, unavailable: string[]) {
    this.platform = platform;
    this.unavailable = unavailable;
  }

  async load(transport: Transport) {
    const info = await transport.call('app.info', {});
    this.set(info.platform, info.unavailable);
  }
}

export const capabilities = new Capabilities();
