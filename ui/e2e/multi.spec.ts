import { expect, test, type Page } from '@playwright/test';
import { E2E_ADMIN, MULTI_PORT } from '../playwright.config';

const base = `http://127.0.0.1:${MULTI_PORT}`;
test.use({ locale: 'en-US', baseURL: base });

async function signIn(page: Page, username: string, password: string) {
  await page.goto('/');
  await page.getByRole('textbox', { name: 'User name' }).fill(username);
  await page.getByRole('textbox', { name: 'Password' }).fill(password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page.getByRole('combobox', { name: 'Workspace' })).toBeVisible();
}

test('users register, share a workspace and viewers cannot change it', async ({ browser }) => {
  // Bob registers himself.
  const bobContext = await browser.newContext();
  const bob = await bobContext.newPage();
  await bob.goto('/');
  await bob.getByRole('button', { name: 'No account? Create one' }).click();
  await bob.getByRole('textbox', { name: 'User name' }).fill('bob');
  await bob.getByRole('textbox', { name: 'Password' }).fill('short');
  await bob.getByRole('button', { name: 'Create account' }).click();
  await expect(bob.getByRole('alert')).toHaveText('Use at least 8 characters');
  await bob.getByRole('textbox', { name: 'Password' }).fill('bob-password');
  await bob.getByRole('button', { name: 'Create account' }).click();
  await expect(bob.getByRole('combobox', { name: 'Workspace' })).toBeVisible();

  // The administrator shares their workspace with Bob as a viewer.
  const adminContext = await browser.newContext();
  const admin = await adminContext.newPage();
  await admin.goto('/');
  await admin.getByRole('textbox', { name: 'User name' }).fill(E2E_ADMIN.username);
  await admin.getByRole('textbox', { name: 'Password' }).fill('wrong-password');
  await admin.getByRole('button', { name: 'Sign in' }).click();
  await expect(admin.getByRole('alert')).toHaveText('Wrong user name or password');
  await signIn(admin, E2E_ADMIN.username, E2E_ADMIN.password);
  const [own] = (await (
    await admin.request.post(`${base}/api/v1/cmd/workspace.list`, { data: {} })
  ).json()) as { id: string }[];
  await admin.request.post(`${base}/api/v1/cmd/workspace.rename`, {
    data: { id: own!.id, name: 'Team' },
  });
  await admin.reload();
  await admin.getByRole('button', { name: 'Workspace actions' }).click();
  await admin.getByRole('menuitem', { name: 'Members…' }).click();
  const members = admin.getByRole('dialog');
  await members.getByRole('textbox', { name: 'User name' }).fill('bob');
  await members.getByRole('combobox', { name: 'Role', exact: true }).last().click();
  await admin.getByRole('option', { name: /Viewer/ }).click();
  await members.getByRole('button', { name: 'Add' }).click();
  await expect(members.getByText('@bob')).toBeVisible();
  await admin.keyboard.press('Escape');

  // Bob sees two workspaces; the shared one is read-only for him.
  await bob.reload();
  await bob.getByRole('combobox', { name: 'Workspace' }).click();
  await expect(bob.getByRole('option')).toHaveCount(2);
  await bob.getByRole('option', { name: 'Team' }).click();
  await expect(bob.getByRole('status').filter({ hasText: 'View only' })).toBeVisible();
  await expect(bob.getByRole('button', { name: 'Create' })).toBeDisabled();

  // The server refuses changes from a viewer even without the UI.
  const workspaces = await bob.request.post(`${base}/api/v1/cmd/workspace.list`, { data: {} });
  const list = (await workspaces.json()) as { id: string; name: string }[];
  expect(list).toHaveLength(2);
  const rename = await Promise.all(
    list.map((w) =>
      bob.request.post(`${base}/api/v1/cmd/workspace.rename`, {
        data: { id: w.id, name: 'Hacked' },
      }),
    ),
  );
  expect(rename.map((r) => r.status()).sort()).toEqual([200, 403]);

  await bobContext.close();
  await adminContext.close();
});

test('an invitation link adds a new user to the workspace', async ({ browser }) => {
  const adminContext = await browser.newContext();
  const admin = await adminContext.newPage();
  await signIn(admin, E2E_ADMIN.username, E2E_ADMIN.password);
  await admin.getByRole('button', { name: 'Workspace actions' }).click();
  await admin.getByRole('menuitem', { name: 'Members…' }).click();
  const members = admin.getByRole('dialog');
  await members.getByRole('button', { name: 'Create link' }).click();
  const link = (await members.locator('.link code').textContent())!;
  expect(link).toContain('?invite=inv_');

  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  await guest.goto(link);
  await expect(guest.getByText(/invited you to “Team” as Editor/)).toBeVisible();
  await guest.getByRole('textbox', { name: 'User name' }).fill('carol');
  await guest.getByRole('textbox', { name: 'Password' }).fill('carol-password');
  await guest.getByRole('button', { name: 'Create account' }).click();
  await guest.getByRole('combobox', { name: 'Workspace' }).click();
  await guest.getByRole('option', { name: 'Team' }).click();
  await expect(guest.getByRole('button', { name: 'Create' })).toBeEnabled();
  expect(new URL(guest.url()).searchParams.get('invite')).toBeNull();

  await guestContext.close();
  await adminContext.close();
});
