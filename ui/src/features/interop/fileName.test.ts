import { describe, expect, it } from 'vitest';
import { stampFileName } from './fileName';

// 27 Nov 2026, 14:05 local time.
const at = new Date(2026, 10, 27, 14, 5, 9);

describe('export file names', () => {
  it('puts the local date and time before the extensions', () => {
    expect(stampFileName('my-api.reqly.json', at)).toBe('my-api-2026-11-27_14-05.reqly.json');
    expect(stampFileName('shop.postman_collection.json', at)).toBe(
      'shop-2026-11-27_14-05.postman_collection.json',
    );
    expect(stampFileName('requests.har', at)).toBe('requests-2026-11-27_14-05.har');
  });

  it('appends to a name without an extension and keeps folders', () => {
    expect(stampFileName('export', at)).toBe('export-2026-11-27_14-05');
    expect(stampFileName('env/dev.reqly.yaml', at)).toBe('env/dev-2026-11-27_14-05.reqly.yaml');
  });

  it('keeps a leading dot (hidden file names)', () => {
    expect(stampFileName('.reqly.json', at)).toBe('.reqly-2026-11-27_14-05.json');
  });
});
