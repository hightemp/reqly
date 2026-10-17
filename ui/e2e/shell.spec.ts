import { expect, test } from '@playwright/test';

test.use({ locale: 'en-US', colorScheme: 'dark' });

test('renders the app shell from the server', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('complementary', { name: 'Collections' })).toBeVisible();
  await expect(page.getByText('Select or create a request')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('preferences persist on the server across reloads', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: /^Language/ }).click();
  await expect(page.getByText('Выберите или создайте запрос')).toBeVisible();

  await page.keyboard.press('Control+b');
  await expect(page.getByRole('complementary')).toBeHidden();

  await page.reload();
  await expect(page.getByText('Выберите или создайте запрос')).toBeVisible();
  await expect(page.getByRole('complementary')).toBeHidden();

  // Restore for other tests.
  await page.getByRole('button', { name: /^Язык/ }).click();
  await page.keyboard.press('Control+b');
  await expect(page.getByRole('complementary')).toBeVisible();
});

test('settings changes propagate to other tabs over WebSocket', async ({ page, context }) => {
  await page.goto('/');
  const other = await context.newPage();
  await other.goto('/');
  await expect(other.getByRole('complementary')).toBeVisible();

  await page.keyboard.press('Control+b');
  await expect(other.getByRole('complementary')).toBeHidden();
  await page.keyboard.press('Control+b');
  await expect(other.getByRole('complementary')).toBeVisible();
});
