import { createServer, type Server } from 'node:http';
import { _android as android, expect, test, type AndroidDevice, type Page } from '@playwright/test';

const PACKAGE = 'dev.reqly.app';
const PORT = 8499;

let server: Server;
let device: AndroidDevice;
let page: Page;

test.beforeAll(async () => {
  // The emulator reaches the host as 10.0.2.2.
  server = createServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ pong: req.url }));
  });
  await new Promise<void>((ok) => server.listen(PORT, '0.0.0.0', ok));

  const [first] = await android.devices();
  if (!first) throw new Error('no Android device or emulator found (adb devices)');
  device = first;
  await device.shell(`am force-stop ${PACKAGE}`);
  await device.shell(`am start -n ${PACKAGE}/.MainActivity`);
  const webview = await device.webView({ pkg: PACKAGE }, { timeout: 60_000 });
  page = await webview.page();
});

test.afterAll(async () => {
  await device?.close();
  server?.close();
});

test('the app starts, sends a request and shows the response', async () => {
  await expect(page.getByRole('button', { name: 'Menu' })).toBeVisible({ timeout: 60_000 });

  await page.getByRole('button', { name: 'Menu' }).click();
  const drawer = page.getByRole('dialog', { name: 'Collections' });
  await drawer.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await expect(drawer).toBeHidden();

  await page.getByRole('textbox', { name: 'URL' }).fill(`http://10.0.2.2:${PORT}/ping`);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Response', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('/ping');
});

test('features Android cannot offer are explained', async () => {
  await page.getByRole('button', { name: 'Request', exact: true }).click();
  await page.getByRole('tab', { name: 'Settings' }).click();
  await expect(page.getByRole('textbox', { name: 'Unix domain socket' })).toBeDisabled();
  await expect(page.getByText('Not available on Android').first()).toBeVisible();
});
