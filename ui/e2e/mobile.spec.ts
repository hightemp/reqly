import { devices, expect, test } from '@playwright/test';

// Phone-sized screen with touch input (the Android layout).
const { defaultBrowserType: _, ...pixel } = devices['Pixel 7'];
test.use({ ...pixel, locale: 'en-US' });

test('compact layout: drawer, open requests and request / response switch', async ({
  page,
  baseURL,
}) => {
  await page.goto('/');
  await expect(page.getByRole('complementary', { name: 'Collections' })).toBeHidden();

  await page.getByRole('button', { name: 'Menu' }).click();
  const drawer = page.getByRole('dialog', { name: 'Collections' });
  await expect(drawer).toBeVisible();
  await drawer.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await expect(drawer).toBeHidden();

  const url = page.getByRole('textbox', { name: 'URL' });
  await url.fill(`${baseURL}/healthz`);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Response', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.getByText('200 OK')).toBeVisible();

  await page.getByRole('button', { name: 'Request', exact: true }).click();
  await expect(url).toBeVisible();

  const open = page.getByRole('button', { name: /^Open requests: \d+$/ });
  await open.click();
  const list = page.getByRole('dialog', { name: 'Open requests' });
  await expect(list.getByRole('button', { name: /New Request/ }).first()).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(list).toBeHidden();

  // Nothing sticks out of the screen.
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
});
