import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { expect, test } from '@playwright/test';

test.use({ locale: 'en-US' });

// A minimal OAuth 2.0 provider: authorize redirects back with a code, the
// token endpoint trades it, the resource echoes the Authorization header.
let provider: Server;
let base = '';

test.beforeAll(async () => {
  provider = createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://x');
    if (url.pathname === '/authorize') {
      const back = new URL(url.searchParams.get('redirect_uri')!);
      back.searchParams.set('code', 'code-1');
      back.searchParams.set('state', url.searchParams.get('state')!);
      res.writeHead(302, { location: back.toString() }).end();
      return;
    }
    if (url.pathname === '/token' && req.method === 'POST') {
      let body = '';
      req.on('data', (c) => (body += c));
      req.on('end', () => {
        const form = new URLSearchParams(body);
        const ok = form.get('code') === 'code-1' && form.get('code_verifier');
        res.writeHead(ok ? 200 : 400, { 'content-type': 'application/json' });
        res.end(
          JSON.stringify(
            ok
              ? { access_token: 'e2e-access', token_type: 'Bearer', expires_in: 3600 }
              : { error: 'invalid_grant' },
          ),
        );
      });
      return;
    }
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ authorization: req.headers.authorization ?? null }));
  });
  await new Promise<void>((r) => provider.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${(provider.address() as AddressInfo).port}`;
});

test.afterAll(() => provider.close());

test('authorization code sign-in through a pop-up', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toBeVisible();
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await page.getByRole('textbox', { name: 'URL' }).fill(`${base}/resource`);

  await page.getByRole('tab', { name: 'Auth' }).click();
  await page.getByRole('combobox', { name: 'Type' }).click();
  await page.getByRole('option', { name: 'OAuth 2.0' }).click();
  await page.getByRole('textbox', { name: 'Authorization URL' }).fill(`${base}/authorize`);
  await page.getByRole('textbox', { name: 'Token URL' }).fill(`${base}/token`);
  await page.getByRole('textbox', { name: 'Client ID' }).fill('reqly');
  await expect(page.getByText(/Register this redirect URL/)).toContainText('/oauth2/callback');

  // Let autosave store the settings the server signs in with.
  await page.waitForTimeout(800);
  const popup = page.waitForEvent('popup');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await (await popup).waitForEvent('close');
  await expect(page.getByText('e2e-access')).toBeVisible();
  await expect(page.getByText('Valid', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('Bearer e2e-access');

  // Leave no open tabs behind for the next specs.
  await page
    .getByRole('tablist', { name: 'Open requests' })
    .getByRole('tab')
    .first()
    .click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Close all' }).click();
  await expect(page.getByRole('tablist', { name: 'Open requests' })).toBeHidden();
  // Open tabs are saved shortly after changing.
  await page.waitForTimeout(800);
});
