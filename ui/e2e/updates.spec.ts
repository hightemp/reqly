import { expect, test } from '@playwright/test';
import { UPDATES_PORT } from '../playwright.config';

const base = `http://127.0.0.1:${UPDATES_PORT}`;

test.use({ locale: 'en-US' });

test('the server admin hears about a newer release', async ({ page }) => {
  await page.goto(base);
  // The automatic check at start finds v99.0.0.
  await expect(page.getByText('Reqly 99.0.0 is available', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Details' }).click();
  const dialog = page.getByRole('dialog', { name: 'Update available' });
  await expect(dialog.getByText(/Reqly 99\.0\.0 is available\. You have /)).toBeVisible();
  await expect(dialog.getByText('- Everything')).toBeVisible();
  await expect(dialog.getByText('docker pull ghcr.io/hightemp/reqly-server:99.0.0')).toBeVisible();
  await dialog.getByRole('button', { name: 'Later' }).click();
  await expect(dialog).toBeHidden();

  // The status bar keeps the update at hand.
  await page.getByRole('button', { name: 'Update 99.0.0' }).click();
  await expect(page.getByRole('dialog', { name: 'Update available' })).toBeVisible();
  await page.keyboard.press('Escape');

  // Settings → About: check again by hand.
  await page.getByRole('button', { name: 'Settings', exact: true }).first().click();
  const settings = page.getByRole('dialog', { name: 'Settings' });
  await settings.getByRole('button', { name: /About/ }).click();
  await settings.getByRole('button', { name: 'Check for updates' }).click();
  await expect(settings.getByText('Reqly 99.0.0 is available', { exact: true })).toBeVisible();
});
