import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';

test.use({ locale: 'en-US' });

const fixture = (path: string) =>
  new URL(`../../crates/reqly-interop/fixtures/${path}`, import.meta.url).pathname;

async function openImport(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toBeVisible();
  await page.getByRole('button', { name: 'Workspace actions' }).click();
  await page.getByRole('menuitem', { name: 'Import…' }).click();
  return page.getByRole('dialog', { name: 'Import' });
}

async function backToDefault(page: Page) {
  const tab = page.getByRole('tablist', { name: 'Open requests' }).getByRole('tab').first();
  if (await tab.isVisible().catch(() => false)) {
    await tab.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Close all' }).click();
  }
  await page.getByRole('combobox', { name: 'Collection' }).click();
  await page.getByRole('option', { name: 'My Collection' }).click();
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(/My Collection/);
  await page.waitForTimeout(300);
}

test.afterEach(async ({ page }) => backToDefault(page));

test('import an Insomnia export from a file, with the report', async ({ page }) => {
  const dialog = await openImport(page);
  const chooser = page.waitForEvent('filechooser');
  await dialog.getByRole('button', { name: 'Choose files…' }).click();
  await (await chooser).setFiles(fixture('insomnia/v4-full.json'));

  await expect(dialog.getByRole('combobox', { name: 'Format' })).toHaveText(/Insomnia/);
  const tree = dialog.getByRole('list', { name: 'What will be imported' });
  await expect(tree).toContainText('Example API');
  await expect(tree).toContainText('List users');
  const report = dialog.getByRole('region', { name: 'Report' });
  await expect(report).not.toContainText('digest');
  await expect(report).toContainText('the .proto file of this gRPC request is not in the export');

  await dialog.getByRole('button', { name: 'Import', exact: true }).click();
  await expect(dialog).toContainText('Imported 10 requests');
  await dialog.getByRole('button', { name: 'Close' }).click();
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(/Example API/);
  await expect(page.getByRole('treeitem', { name: /Users/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /Environment:/ })).toBeVisible();
});

test('paste a Postman collection, export it and import it again', async ({ page }) => {
  const dialog = await openImport(page);
  await dialog.getByRole('tab', { name: 'Paste' }).click();
  await dialog
    .getByRole('textbox', { name: 'Text to import' })
    .fill(readFileSync(fixture('postman/full-v2_1.json'), 'utf8'));
  await dialog.getByRole('button', { name: 'Continue' }).click();
  await expect(dialog.getByRole('combobox', { name: 'Format' })).toHaveText(/Postman/);
  await dialog.getByRole('button', { name: 'Import', exact: true }).click();
  await dialog.getByRole('button', { name: 'Close' }).click();
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(/Shop API/);

  // Export the collection as Reqly JSON (a browser download).
  await page.getByRole('button', { name: 'Collection actions' }).click();
  await page.getByRole('menuitem', { name: 'Export collection…' }).click();
  const exportDialog = page.getByRole('dialog', { name: /Export/ });
  await exportDialog.getByRole('combobox', { name: 'Format' }).click();
  await page.getByRole('option', { name: 'Reqly (JSON)' }).click();
  const download = page.waitForEvent('download');
  await exportDialog.getByRole('button', { name: 'Export', exact: true }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('shop-api.reqly.json');
  const content = readFileSync(await file.path(), 'utf8');
  expect(JSON.parse(content).format).toBe('reqly');

  // …and import it back into the same collection with "merge": nothing is duplicated.
  const again = await openImport(page);
  await again.getByRole('tab', { name: 'Paste' }).click();
  await again.getByRole('textbox', { name: 'Text to import' }).fill(content);
  await again.getByRole('button', { name: 'Continue' }).click();
  await again.getByRole('combobox', { name: 'Import into' }).click();
  await page.getByRole('option', { name: 'Shop API' }).last().click();
  await again.getByRole('button', { name: 'Import', exact: true }).click();
  await again.getByRole('button', { name: 'Close' }).click();
  await expect(page.getByRole('treeitem', { name: /Products/ })).toHaveCount(1);
  await expect(page.getByRole('treeitem', { name: /Create order/ })).toHaveCount(1);
});

test('generate code for a request', async ({ page }) => {
  const dialog = await openImport(page);
  await dialog.getByRole('tab', { name: 'Paste' }).click();
  await dialog
    .getByRole('textbox', { name: 'Text to import' })
    .fill(
      "curl -X POST https://api.example.com/items -H 'Content-Type: application/json' -d '{\"a\":1}'",
    );
  await dialog.getByRole('button', { name: 'Continue' }).click();
  await dialog.getByRole('button', { name: 'Import', exact: true }).click();
  await dialog.getByRole('button', { name: 'Close' }).click();

  await page.getByRole('treeitem', { name: /POST \/items/ }).click();
  await page.getByRole('button', { name: 'Generate code…' }).click();
  const codegen = page.getByRole('dialog', { name: 'Generate code' });
  await codegen.getByRole('combobox', { name: 'Language' }).click();
  await page.getByRole('option', { name: 'Python — Requests' }).click();
  await expect(codegen.locator('.cm-content')).toContainText('requests.request("POST", url');
  await expect(codegen.locator('.cm-content')).toContainText('https://api.example.com/items');
  await codegen.getByRole('button', { name: 'Close' }).click();
});

test('import requests from HTTP text', async ({ page }) => {
  const dialog = await openImport(page);
  await dialog.getByRole('tab', { name: 'Paste' }).click();
  await dialog
    .getByRole('textbox', { name: 'Text to import' })
    .fill(
      [
        '@base = https://api.example.com',
        '### List orders',
        'GET {{base}}/orders',
        'Accept: application/json',
        '',
        '###',
        'POST {{base}}/orders',
        'Content-Type: application/json',
        '',
        '{"item": 1}',
      ].join('\n'),
    );
  await dialog.getByRole('button', { name: 'Continue' }).click();
  await expect(dialog.getByRole('combobox', { name: 'Format' })).toContainText('HTTP (.http)');
  await dialog.getByRole('button', { name: 'Import', exact: true }).click();
  await dialog.getByRole('button', { name: 'Close' }).click();
  await expect(page.getByRole('treeitem', { name: /List orders/ })).toHaveCount(1);
  await expect(page.getByRole('treeitem', { name: /POST \/orders/ })).toHaveCount(1);
});

test('supported formats window', async ({ page }) => {
  const dialog = await openImport(page);
  await dialog.getByRole('button', { name: 'Supported formats' }).click();
  const help = page.getByRole('dialog', { name: 'Supported formats' });
  await expect(help.getByRole('heading', { name: 'HTTP (.http / .rest)' })).toBeVisible();
  await expect(help.getByLabel('Example: cURL')).toContainText('curl -X POST');
  await help.getByRole('button', { name: 'Close' }).click();
  await expect(help).toBeHidden();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
});

test('paste requests from the clipboard and import into a folder', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/');
  const name = `Inbox ${Date.now().toString(36)}`;
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New folder' }).click();
  const input = page.getByRole('tree', { name: 'Collections' }).getByRole('textbox');
  await input.fill(name);
  await input.press('Enter');
  const folder = page.getByRole('treeitem', { name: new RegExp(name) });
  await expect(folder).toBeVisible();

  await page.evaluate(() =>
    navigator.clipboard.writeText(
      'curl https://api.example.com/pasted-a\ncurl -X POST https://api.example.com/pasted-b',
    ),
  );
  await folder.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Import from clipboard' }).click();
  await expect(page.getByText('Added 2 requests')).toBeVisible();
  await expect(page.getByRole('treeitem', { name: /GET \/pasted-a/ })).toBeVisible();
  await expect(page.getByRole('treeitem', { name: /POST \/pasted-b/ })).toBeVisible();

  await folder.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Import into this folder…' }).click();
  const dialog = page.getByRole('dialog', { name: 'Import' });
  await dialog.getByRole('tab', { name: 'Paste' }).click();
  await dialog
    .getByRole('textbox', { name: 'Text to import' })
    .fill('GET https://api.example.com/imported HTTP/1.1');
  await dialog.getByRole('button', { name: 'Continue' }).click();
  await expect(dialog.getByRole('combobox', { name: 'Import into' })).toContainText(name);
  await dialog.getByRole('button', { name: 'Import', exact: true }).click();
  await dialog.getByRole('button', { name: 'Close' }).click();
  const imported = page.getByRole('treeitem', { name: /GET \/imported/ });
  await expect(imported).toBeVisible();
  // Inside the folder: one level deeper than the folder.
  await expect(imported).toHaveAttribute(
    'aria-level',
    String(Number(await folder.getAttribute('aria-level')) + 1),
  );
});

test('import from clipboard falls back to the wizard when the clipboard is not readable', async ({
  page,
}) => {
  // As in WebKitGTK (the Linux app without the plugin) or Firefox.
  await page.addInitScript(() => {
    navigator.clipboard.readText = () =>
      Promise.reject(new DOMException('denied', 'NotAllowedError'));
  });
  await page.goto('/');
  const name = `Denied ${Date.now().toString(36)}`;
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New folder' }).click();
  const input = page.getByRole('tree', { name: 'Collections' }).getByRole('textbox');
  await input.fill(name);
  await input.press('Enter');
  const folder = page.getByRole('treeitem', { name: new RegExp(name) });
  await folder.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Import from clipboard' }).click();
  const dialog = page.getByRole('dialog', { name: 'Import' });
  await expect(dialog.getByRole('tab', { name: 'Paste' })).toHaveAttribute('aria-selected', 'true');
  await expect(dialog.getByRole('textbox', { name: 'Text to import' })).toBeFocused();
  await dialog.getByRole('textbox', { name: 'Text to import' }).fill('curl https://x.test/denied');
  await dialog.getByRole('button', { name: 'Continue' }).click();
  await expect(dialog.getByRole('combobox', { name: 'Import into' })).toContainText(name);
  await dialog.getByRole('button', { name: 'Back' }).click();
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toBeHidden();
});
