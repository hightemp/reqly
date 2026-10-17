import { expect, test } from '@playwright/test';
import { E2E_TOKEN, TOKEN_PORT } from '../playwright.config';

const base = `http://127.0.0.1:${TOKEN_PORT}`;
test.use({ locale: 'en-US', baseURL: base });

test('token mode requires login and keeps the session', async ({ page, request }) => {
  const anonymous = await request.post(`${base}/api/v1/cmd/app.info`, { data: {} });
  expect(anonymous.status()).toBe(401);

  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Sign in to Reqly' })).toBeVisible();
  await page.getByRole('textbox', { name: 'Access token' }).fill('wrong-token');
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page.getByRole('alert')).toHaveText('The token is not valid');

  await page.getByRole('textbox', { name: 'Access token' }).fill(E2E_TOKEN);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page.getByRole('combobox', { name: 'Workspace' })).toBeVisible();

  await page.reload();
  await expect(page.getByRole('combobox', { name: 'Workspace' })).toBeVisible();

  const bearer = await request.post(`${base}/api/v1/cmd/app.info`, {
    data: {},
    headers: { authorization: `Bearer ${E2E_TOKEN}` },
  });
  expect(bearer.status()).toBe(200);
});
