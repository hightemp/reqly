import { expect, test, type Page } from '@playwright/test';

test.use({ locale: 'en-US' });

async function newRequest(page: Page, url: string, expectedStatus: string) {
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const field = page.getByRole('textbox', { name: 'URL' });
  await expect(async () => {
    await field.fill(url);
    await expect(field).toHaveText(url, { timeout: 1000 });
  }).toPass();
  await page
    .getByRole('tablist', { name: 'Request sections' })
    .getByRole('tab', { name: /^Tests/ })
    .click();
  await page.getByRole('button', { name: 'Add assertion' }).click();
  const value = page.getByRole('textbox', { name: 'Expected value' });
  await value.fill(expectedStatus);
  await expect(value).toHaveText(expectedStatus);
  await page.waitForTimeout(800);
}

test('collection runner', async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await page.goto('/');
  const name = `Runner ${Date.now().toString(36)}`;
  await page.getByRole('button', { name: 'Collection actions' }).click();
  await page.getByRole('menuitem', { name: 'New collection' }).click();
  const create = page.getByRole('dialog', { name: 'New collection' });
  await create.getByRole('textbox', { name: 'Name' }).fill(name);
  await create.getByRole('button', { name: 'OK' }).click();
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(new RegExp(name));

  await newRequest(page, `${baseURL}/healthz?n=1`, '200');
  await newRequest(page, `${baseURL}/healthz?n=2`, '201');

  await page.getByRole('button', { name: 'Collection actions' }).click();
  await page.getByRole('menuitem', { name: 'Run collection…' }).click();
  const dialog = page.getByRole('dialog', { name: `Run ${name}` });
  await expect(dialog.getByRole('list', { name: 'Requests' }).getByRole('listitem')).toHaveCount(2);
  await dialog.getByRole('spinbutton', { name: 'Iterations' }).fill('2');
  await dialog.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(dialog.getByText('2 requests passed')).toBeVisible();
  await expect(dialog.getByText('2 requests failed')).toBeVisible();
  await expect(dialog.getByText('Iteration 2')).toBeVisible();

  await dialog.getByRole('button', { name: 'Rerun failed' }).click();
  const results = dialog.getByRole('list', { name: 'Results' });
  await expect(results.getByRole('button')).toHaveCount(2);
  await expect(dialog.getByText('2 requests failed')).toBeVisible();
  await results.getByRole('button').first().click();
  await expect(dialog.getByText('Got: 200')).toBeVisible();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toBeHidden();

  // Clean up tabs for the next specs.
  await page
    .getByRole('tablist', { name: 'Open requests' })
    .getByRole('tab')
    .first()
    .click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Close all' }).click();
  await page.waitForTimeout(800);
});
