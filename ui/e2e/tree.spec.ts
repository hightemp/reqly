import { expect, test, type Page } from '@playwright/test';

test.use({ locale: 'en-US' });

async function create(page: Page, kind: 'New request' | 'New folder', name: string) {
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: kind }).click();
  if (kind === 'New folder') {
    const input = page.getByRole('tree', { name: 'Collections' }).getByRole('textbox');
    await input.fill(name);
    await input.press('Enter');
  } else {
    const item = page.getByRole('treeitem', { name: /New Request/ }).last();
    await item.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Rename' }).click();
    const input = page.getByRole('tree', { name: 'Collections' }).getByRole('textbox');
    await input.fill(name);
    await input.press('Enter');
  }
  await expect(page.getByRole('treeitem', { name: new RegExp(name) })).toBeVisible();
}

test('organize requests: cut/paste, drag & drop, search and the list view', async ({ page }) => {
  await page.goto('/');
  const suffix = Date.now().toString(36);
  // A fresh collection keeps the tree small (no autoscroll while dragging).
  await page.getByRole('button', { name: 'Collection actions' }).click();
  await page.getByRole('menuitem', { name: 'New collection' }).click();
  const dialog = page.getByRole('dialog', { name: 'New collection' });
  await dialog.getByRole('textbox', { name: 'Name' }).fill(`Tree ${suffix}`);
  await dialog.getByRole('button', { name: 'OK' }).click();
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(
    new RegExp(`Tree ${suffix}`),
  );
  const folder = `Billing ${suffix}`;
  const other = `Archive ${suffix}`;
  const req = `Invoice ${suffix}`;
  await create(page, 'New folder', folder);
  // Escape in the tree clears the selection, so the next folder goes to the root.
  await page.getByRole('treeitem', { name: new RegExp(folder) }).click();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('treeitem', { selected: true })).toHaveCount(0);
  await create(page, 'New folder', other);
  await create(page, 'New request', req);

  // Cut + paste into the first folder via the context menu.
  await page.getByRole('treeitem', { name: new RegExp(req) }).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Cut' }).click();
  await page.getByRole('treeitem', { name: new RegExp(folder) }).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Paste' }).click();
  const folderRow = page.getByRole('treeitem', { name: new RegExp(folder) });
  await expect(folderRow).toHaveAttribute('aria-expanded', 'true');
  const reqRow = page.getByRole('treeitem', { name: new RegExp(req) });
  const folderLevel = Number(await folderRow.getAttribute('aria-level'));
  await expect(reqRow).toHaveAttribute('aria-level', String(folderLevel + 1));

  // Drag the request onto the other folder.
  const target = page.getByRole('treeitem', { name: new RegExp(other) });
  const from = (await reqRow.boundingBox())!;
  const to = (await target.boundingBox())!;
  await page.mouse.move(from.x + 40, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + 40, from.y + from.height / 2 + 12, { steps: 3 });
  await page.mouse.move(to.x + 40, to.y + to.height / 2, { steps: 8 });
  await expect(target).toHaveClass(/drop-inside/);
  await page.mouse.up();
  await expect(target).toHaveAttribute('aria-expanded', 'true');
  // One level below the folder it was dropped into.
  const targetLevel = Number(await target.getAttribute('aria-level'));
  await expect(page.getByRole('treeitem', { name: new RegExp(req) })).toHaveAttribute(
    'aria-level',
    String(targetLevel + 1),
  );
  // Its parent is the closest row above with a lower level.
  const rows = await page
    .getByRole('tree', { name: 'Collections' })
    .getByRole('treeitem')
    .evaluateAll((els) =>
      els.map((e) => ({ text: e.textContent ?? '', level: Number(e.getAttribute('aria-level')) })),
    );
  const at = rows.findIndex((r) => r.text.includes(req));
  const parent = rows
    .slice(0, at)
    .reverse()
    .find((r) => r.level === rows[at]!.level - 1);
  expect(parent?.text).toContain(other);

  // Search narrows the tree to matches and their folders.
  await page.getByRole('textbox', { name: 'Search requests' }).fill(`invoice ${suffix}`);
  // The match and each of its ancestor folders.
  const level = Number(
    await page.getByRole('treeitem', { name: new RegExp(req) }).getAttribute('aria-level'),
  );
  await expect(page.getByRole('treeitem')).toHaveCount(level);
  await page.getByRole('textbox', { name: 'Search requests' }).fill('');

  // Folders + list view: selecting the folder lists its requests.
  await page.getByRole('button', { name: 'Show folders and list' }).click();
  await page
    .getByRole('tree', { name: 'Folders' })
    .getByRole('treeitem', { name: new RegExp(other) })
    .click();
  await expect(
    page.getByRole('tree', { name: 'Requests' }).getByRole('treeitem', { name: new RegExp(req) }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Show as tree' }).click();
});

test('deleting a request or its folder closes its open tab', async ({ page }) => {
  await page.goto('/');
  const suffix = Date.now().toString(36);
  const folder = `Doomed ${suffix}`;
  const inner = `Inner ${suffix}`;
  const lone = `Lone ${suffix}`;
  await create(page, 'New folder', folder);
  await create(page, 'New request', inner);
  await page.getByRole('treeitem', { name: new RegExp(inner) }).click();
  await page.keyboard.press('Escape');
  await create(page, 'New request', lone);

  const tabs = page.getByRole('tablist', { name: 'Open requests' });
  const tree = page.getByRole('tree', { name: 'Collections' });
  const remove = async (name: string) => {
    await tree.getByRole('treeitem', { name: new RegExp(name) }).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Delete' }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Delete' }).click();
  };

  // Both requests are open in permanent tabs.
  for (const name of [inner, lone]) {
    await tree.getByRole('treeitem', { name: new RegExp(name) }).dblclick();
    await expect(tabs.getByRole('tab', { name: new RegExp(name) })).toBeVisible();
  }

  await remove(lone);
  await expect(tabs.getByRole('tab', { name: new RegExp(lone) })).toHaveCount(0);
  await expect(tabs.getByRole('tab', { name: new RegExp(inner) })).toBeVisible();

  // A deleted folder closes the tabs of the requests inside it.
  await remove(folder);
  await expect(tabs.getByRole('tab', { name: new RegExp(inner) })).toHaveCount(0);
});

test('Delete while renaming edits the name instead of deleting the request', async ({ page }) => {
  await page.goto('/');
  const name = `Delkey ${Date.now().toString(36)}`;
  await create(page, 'New request', `X${name}`);
  const row = page.getByRole('treeitem', { name: new RegExp(`X${name}`) });
  await row.click();
  await page.keyboard.press('F2');
  const input = page.getByRole('tree', { name: 'Collections' }).getByRole('textbox');
  await expect(input).toHaveValue(`X${name}`);
  await input.press('Home');
  await input.press('Delete');
  await expect(input).toHaveValue(name);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await input.press('Enter');
  await expect(page.getByRole('treeitem', { name: new RegExp(`^.*${name}`) })).toBeVisible();
  await expect(page.getByRole('treeitem', { name: new RegExp(`X${name}`) })).toHaveCount(0);
});
