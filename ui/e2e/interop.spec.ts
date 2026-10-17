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
