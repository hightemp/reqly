import { devices, expect, test, type Page } from '@playwright/test';

// Phone-sized screen with touch input (the Android layout).
const { defaultBrowserType: _, ...pixel } = devices['Pixel 7'];
test.use({ ...pixel, locale: 'en-US' });

/** Closes every open request so the next specs start without tabs. */
async function closeAllRequests(page: Page) {
  await page.reload();
  await page.getByRole('button', { name: /^Open requests: \d+$/ }).click();
  const list = page.getByRole('dialog', { name: 'Open requests' });
  const close = list.getByRole('button', { name: 'Close', exact: true });
  while ((await close.count()) > 0) await close.first().click();
  await expect(list.getByText('No open requests')).toBeVisible();
  await page.keyboard.press('Escape');
}

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
  await closeAllRequests(page);
});

test('touch: finger-sized rows and long press for the context menu', async ({ page }) => {
  await page.goto('/');
  const drawer = page.getByRole('dialog', { name: 'Collections' });
  await page.getByRole('button', { name: 'Menu' }).click();
  await drawer.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await expect(drawer).toBeHidden();
  await page.getByRole('button', { name: 'Menu' }).click();
  const row = drawer.getByRole('treeitem').first();
  await expect(row).toBeVisible();
  // Let the drawer finish sliding in.
  await page.waitForTimeout(300);
  const box = (await row.boundingBox())!;
  expect(box.height).toBeGreaterThanOrEqual(40);

  // Hold a finger on the row.
  const cdp = await page.context().newCDPSession(page);
  const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point] });
  await page.waitForTimeout(800);
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  const menu = page.getByRole('menu');
  await expect(menu).toBeVisible();
  await expect(menu.getByRole('menuitem', { name: 'Rename' })).toBeVisible();
  // Releasing the finger does not pick a menu item.
  await expect(drawer).toBeVisible();
  await closeAllRequests(page);
});
