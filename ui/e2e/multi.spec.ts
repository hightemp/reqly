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

test('administrators manage users in the admin panel', async ({ browser }) => {
  const context = await browser.newContext();
  const admin = await context.newPage();
  await signIn(admin, E2E_ADMIN.username, E2E_ADMIN.password);
  await admin.getByRole('complementary').getByRole('button', { name: 'Settings' }).click();
  const settings = admin.getByRole('dialog');
  await settings.getByRole('button', { name: 'Administration' }).click();
  await settings.getByPlaceholder('User name').fill('dave');
  await settings.getByPlaceholder('Password').fill('dave-password');
  await settings.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(settings.getByText('@dave')).toBeVisible();
  await settings.getByRole('checkbox', { name: 'dave can sign in' }).click();
  await expect(settings.getByRole('checkbox', { name: 'dave can sign in' })).not.toBeChecked();

  await settings.getByRole('tab', { name: 'Server' }).click();
  await expect(settings.getByText('multi', { exact: true })).toBeVisible();

  const guest = await (await browser.newContext()).newPage();
  await guest.goto('/');
  await guest.getByRole('textbox', { name: 'User name' }).fill('dave');
  await guest.getByRole('textbox', { name: 'Password' }).fill('dave-password');
  await guest.getByRole('button', { name: 'Sign in' }).click();
  await expect(guest.getByRole('alert')).toHaveText(
    'This account is disabled. Ask an administrator.',
  );
  await context.close();
});

test('changes by one member appear for the others without reloading', async ({ browser }) => {
  const ownerContext = await browser.newContext();
  const owner = await ownerContext.newPage();
  await signIn(owner, E2E_ADMIN.username, E2E_ADMIN.password);
  const api = (name: string, data: unknown) =>
    owner.request
      .post(`${base}/api/v1/cmd/${name}`, { data })
      .then((r) => r.json() as Promise<Record<string, unknown> & { id: string }>);
  const ws = await api('workspace.create', { name: 'Live' });
  const col = await api('collection.create', { workspaceId: ws.id, name: 'Live API' });
  const invite = (await api('members.invite', {
    workspaceId: ws.id,
    role: 'editor',
  })) as unknown as {
    token: string;
  };

  const otherContext = await browser.newContext();
  const other = await otherContext.newPage();
  await other.goto(`/?invite=${invite.token}`);
  await other.getByRole('textbox', { name: 'User name' }).fill('erin');
  await other.getByRole('textbox', { name: 'Password' }).fill('erin-password');
  await other.getByRole('button', { name: 'Create account' }).click();
  await other.getByRole('combobox', { name: 'Workspace' }).click();
  await other.getByRole('option', { name: 'Live' }).click();
  await expect(other.getByRole('combobox', { name: 'Collection' })).toHaveText('Live API');

  await api('node.create', {
    collectionId: col.id,
    parentId: null,
    name: 'Created elsewhere',
    data: { kind: 'request', method: 'GET', url: 'https://example.com' },
  });
  await expect(other.getByRole('treeitem', { name: /Created elsewhere/ })).toBeVisible();

  await ownerContext.close();
  await otherContext.close();
});

test('edits to different fields merge and overwrites are announced', async ({ browser }) => {
  const ownerContext = await browser.newContext();
  const owner = await ownerContext.newPage();
  await signIn(owner, E2E_ADMIN.username, E2E_ADMIN.password);
  const call = (page: Page, name: string, data: unknown) =>
    page.request
      .post(`${base}/api/v1/cmd/${name}`, { data })
      .then((r) => r.json() as Promise<Record<string, never> & { id: string; version: number }>);
  const ws = await call(owner, 'workspace.create', { name: 'Merge' });
  const col = await call(owner, 'collection.create', { workspaceId: ws.id, name: 'Merge API' });
  const req = await call(owner, 'node.create', {
    collectionId: col.id,
    parentId: null,
    name: 'Shared request',
    data: { kind: 'request', method: 'GET', url: 'https://example.com/a' },
  });
  const invite = (await call(owner, 'members.invite', {
    workspaceId: ws.id,
    role: 'editor',
  })) as unknown as { token: string };

  const otherContext = await browser.newContext();
  const other = await otherContext.newPage();
  await other.request.post(`${base}/api/v1/auth/register`, {
    data: {
      username: 'frank',
      password: 'frank-password',
      displayName: 'Frank',
      invite: invite.token,
    },
  });

  await owner.reload();
  await owner.getByRole('combobox', { name: 'Workspace' }).click();
  await owner.getByRole('option', { name: 'Merge' }).click();
  await owner.getByRole('treeitem', { name: /Shared request/ }).click();
  const url = owner.getByRole('textbox', { name: 'URL' });
  await expect(url).toHaveText('https://example.com/a');

  // Frank changes the method; the owner's open editor picks it up.
  await call(other, 'node.update', { id: req.id, baseVersion: 1, fields: { method: 'POST' } });
  await expect(owner.getByRole('combobox', { name: 'Method' })).toHaveValue('POST');

  // The owner edits the URL: only that field is saved, the method stays.
  await url.fill('https://example.com/owner');
  await expect
    .poll(async () => (await call(other, 'node.get', { id: req.id })).data)
    .toMatchObject({ method: 'POST', url: 'https://example.com/owner' });

  // Frank, still on an old version, overwrites the URL: the owner is told.
  // (The URL bar keeps its own text while focused.)
  await url.blur();
  await call(other, 'node.update', {
    id: req.id,
    baseVersion: 2,
    fields: { url: 'https://example.com/frank' },
  });
  await expect(owner.getByText(/Frank overwrote your change to url/)).toBeVisible();
  await expect(url).toHaveText('https://example.com/frank');

  await ownerContext.close();
  await otherContext.close();
});
