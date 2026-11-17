import { expect, test } from '@playwright/test';

test.use({ locale: 'en-US' });

test('help: F1, links, back, search and context help', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toBeVisible();

  await page.keyboard.press('F1');
  const help = page.getByRole('dialog', { name: 'Help' });
  await expect(help.getByRole('heading', { level: 1, name: 'Getting started' })).toBeVisible();

  // A link inside the page, then Back.
  await help.getByRole('article').getByRole('link', { name: 'Variables and environments' }).click();
  await expect(
    help.getByRole('heading', { level: 1, name: 'Variables and environments' }),
  ).toBeVisible();
  await help.getByRole('button', { name: 'Back' }).click();
  await expect(help.getByRole('heading', { level: 1, name: 'Getting started' })).toBeVisible();

  // Search.
  await help.getByRole('searchbox', { name: 'Search help' }).fill('reflection');
  await help.getByRole('navigation', { name: 'Search results' }).getByRole('link').first().click();
  await expect(help.getByRole('heading', { level: 1, name: 'Protocols' })).toBeVisible();

  // The shortcuts page shows the live bindings.
  await help.getByRole('searchbox', { name: 'Search help' }).fill('');
  await help
    .getByRole('navigation', { name: 'Contents' })
    .getByRole('link', { name: 'Keyboard shortcuts' })
    .click();
  await expect(help.getByRole('table', { name: 'Keyboard shortcuts' })).toContainText(
    'Command palette',
  );
  await help.getByRole('button', { name: 'Close' }).click();
  await expect(help).toBeHidden();

  // The status bar button and the help of a dialog.
  await page.getByRole('button', { name: 'Workspace actions' }).click();
  await page.getByRole('menuitem', { name: 'Import…' }).click();
  const wizard = page.getByRole('dialog', { name: 'Import' });
  await wizard.getByRole('button', { name: 'Help on this' }).click();
  await expect(help.getByRole('heading', { level: 1, name: 'Import and export' })).toBeVisible();
  await help.getByRole('button', { name: 'Close' }).click();
  await wizard.getByRole('button', { name: 'Cancel' }).click();

  await page.getByRole('contentinfo').getByRole('button', { name: 'Help' }).click();
  await expect(help).toBeVisible();
  await help.getByRole('button', { name: 'Close' }).click();
});
