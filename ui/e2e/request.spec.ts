import { expect, test } from '@playwright/test';

test.use({ locale: 'en-US', colorScheme: 'dark' });

test('create a request, send it and see the response', async ({ page, baseURL }) => {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(/My Collection/);

  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const tab = page.getByRole('tab', { name: /New Request/ });
  await expect(tab).toBeVisible();

  const url = page.getByRole('textbox', { name: 'URL' });
  await url.fill(`${baseURL}/healthz?probe=1`);
  await expect(page.getByRole('tab', { name: /Params/ })).toContainText('1');
  await page.getByRole('button', { name: 'Send', exact: true }).click();

  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('"status": "ok"');

  await page.getByRole('tab', { name: 'Timeline' }).click();
  await expect(page.getByText(/GET \/healthz\?probe=1 HTTP/)).toBeVisible();

  // Autosave + restored tabs and history after reload.
  await page.waitForTimeout(800);
  await page.reload();
  await expect(page.getByRole('tab', { name: /New Request/ })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'URL' })).toHaveValue(
    `${baseURL}/healthz?probe=1`,
  );
  await expect(page.getByText('200 OK')).toBeVisible();
});

test('pasting a cURL command imports it', async ({ page, baseURL }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const url = page.getByRole('textbox', { name: 'URL' });
  await url.focus();
  await page.evaluate((cmd) => {
    const data = new DataTransfer();
    data.setData('text', cmd);
    document.activeElement!.dispatchEvent(
      new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, `curl -X POST '${baseURL}/api/v1/cmd/app.info' -H 'Content-Type: application/json' -d '{}'`);
  await page.getByRole('button', { name: 'Import' }).click();
  await expect(page.getByRole('combobox', { name: 'Method' })).toHaveValue('POST');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('"platform": "server"');
});

test('folders, rename and delete with undo', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New folder' }).click();
  const rename = page.getByRole('tree', { name: 'Collections' }).getByRole('textbox');
  await rename.fill('Users API');
  await rename.press('Enter');
  const folder = page.getByRole('treeitem', { name: /Users API/ });
  await expect(folder).toBeVisible();

  await folder.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'Delete' }).click();
  await expect(folder).toBeHidden();
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByRole('treeitem', { name: /Users API/ })).toBeVisible();
});
