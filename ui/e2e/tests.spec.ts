import { expect, test } from '@playwright/test';

test.use({ locale: 'en-US' });

test('no-code assertions', async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toBeVisible();
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const url = page.getByRole('textbox', { name: 'URL' });
  await expect(async () => {
    await url.fill(`${baseURL}/healthz`);
    await expect(url).toHaveText(`${baseURL}/healthz`, { timeout: 1000 });
  }).toPass();

  const requestTabs = page.getByRole('tablist', { name: 'Request sections' });
  await requestTabs.getByRole('tab', { name: /^Tests/ }).click();
  await page.getByRole('button', { name: 'Add assertion' }).click();
  await page.getByRole('button', { name: 'Add assertion' }).click();
  const second = page.getByRole('list', { name: 'Assertions' }).getByRole('listitem').nth(1);
  await second.getByRole('combobox', { name: 'Source' }).click();
  await page.getByRole('option', { name: 'JSONPath' }).click();
  await second.getByRole('textbox', { name: 'Property' }).fill('$.status');
  const value = second.getByRole('textbox', { name: 'Expected value' });
  await value.fill('down');
  await expect(value).toHaveText('down');
  await expect(requestTabs.getByRole('tab', { name: /^Tests/ })).toContainText('2');

  await page.waitForTimeout(800);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();
  await page
    .getByRole('tablist', { name: 'Response sections' })
    .getByRole('tab', { name: /^Tests/ })
    .click();
  const results = page.locator('#response-panel-tests');
  await expect(results.getByText('1 passed')).toBeVisible();
  await expect(results.getByText('1 failed')).toBeVisible();
  await expect(results.getByText('JSONPath $.status equals down')).toBeVisible();
  await expect(results.getByText('Got: ok')).toBeVisible();

  // Clean up tabs for the next specs.
  await page
    .getByRole('tablist', { name: 'Open requests' })
    .getByRole('tab')
    .first()
    .click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Close all' }).click();
  await page.waitForTimeout(800);
});
