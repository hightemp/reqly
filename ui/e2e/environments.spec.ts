import { expect, test, type Page } from '@playwright/test';

test.use({ locale: 'en-US' });

async function newRequest(page: Page) {
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await expect(page.getByRole('tab', { name: /New Request/ }).last()).toBeVisible();
}

/** Leaves no open tabs behind for the next spec. */
async function closeAllTabs(page: Page) {
  const tab = page.getByRole('tablist', { name: 'Open requests' }).getByRole('tab').first();
  if (await tab.isVisible().catch(() => false)) {
    await tab.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Close all' }).click();
  }
}

/** Each test works in its own collection so others see no leftovers. */
async function freshCollection(page: Page) {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toBeVisible();
  const name = `Env ${Date.now().toString(36)}`;
  await page.getByRole('button', { name: 'Collection actions' }).click();
  await page.getByRole('menuitem', { name: 'New collection' }).click();
  const dialog = page.getByRole('dialog', { name: 'New collection' });
  await dialog.getByRole('textbox', { name: 'Name' }).fill(name);
  await dialog.getByRole('button', { name: 'OK' }).click();
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(new RegExp(name));
}

test.afterEach(async ({ page }) => {
  await closeAllTabs(page);
  await expect(page.getByRole('tablist', { name: 'Open requests' })).toBeHidden();
  await page.getByRole('combobox', { name: 'Collection' }).click();
  await page.getByRole('option', { name: 'My Collection' }).click();
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(/My Collection/);
  await page.waitForTimeout(300); // let the active collection and tabs persist
});

async function addVariable(page: Page, name: string, value: string) {
  const dialog = page.getByRole('dialog', { name: 'Environments' });
  await dialog.getByRole('textbox', { name: 'New variable name' }).fill(name);
  const values = dialog.getByRole('textbox', { name: 'Value', exact: true });
  await values.last().click();
  await page.keyboard.insertText(value);
}

test('environments: variables, switching, highlighting and send', async ({ page, baseURL }) => {
  await freshCollection(page);

  // Base environment holds the host; the "dev" environment the path.
  await page.keyboard.press('Control+e');
  const dialog = page.getByRole('dialog', { name: 'Environments' });
  await expect(dialog).toBeVisible();
  await addVariable(page, 'host', baseURL!);

  await dialog.getByRole('button', { name: 'New environment' }).click();
  const prompt = page.getByRole('dialog', { name: 'New environment' });
  await prompt.getByRole('textbox').fill('dev');
  await prompt.getByRole('button', { name: 'OK' }).click();
  await expect(dialog.getByRole('option', { name: /dev/ })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  await addVariable(page, 'path', '/healthz');
  await dialog.getByRole('button', { name: 'Activate' }).click();
  await page.waitForTimeout(600); // debounced save
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(page.getByRole('button', { name: 'Environment: dev' })).toBeVisible();

  await newRequest(page);
  const url = page.getByRole('textbox', { name: 'URL' });
  await url.click();
  await page.keyboard.insertText(
    '{{ host }}{{ path }}?id={% uuid %}&who={{ $faker.person.firstName }}&x={{ nope }}',
  );
  const bar = page.locator('.urlbar');
  await expect(bar.locator('.cm-tpl-var.cm-tpl-ok')).toHaveCount(2);
  await expect(bar.locator('.cm-tpl-fn.cm-tpl-ok')).toHaveCount(2);
  await expect(bar.locator('.cm-tpl-missing')).toHaveText('{{ nope }}');

  // Hover shows the value and where it comes from.
  await bar.locator('.cm-tpl-var', { hasText: 'path' }).hover();
  await expect(page.locator('.cm-tpl-preview')).toContainText('/healthz');
  await expect(page.locator('.cm-tpl-preview')).toContainText('Environment: dev');

  // An undefined variable stops the send with a clear message.
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('Variable "nope" is not defined')).toBeVisible();

  await url.click();
  await page.keyboard.press('End');
  for (let i = 0; i < '&x={{ nope }}'.length; i++) await page.keyboard.press('Backspace');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();
  await page.getByRole('tab', { name: 'Timeline' }).click();
  await expect(page.getByText(/GET \/healthz\?id=[0-9a-f-]{36}&who=\w+ HTTP/)).toBeVisible();

  // Switching the environment off makes `path` undefined.
  await page.getByRole('button', { name: 'Environment: dev' }).click();
  await page.getByRole('menuitem', { name: 'No environment' }).first().click();
  await expect(bar.locator('.cm-tpl-var.cm-tpl-missing')).toHaveText('{{ path }}');
});

test('completion after {{ offers variables and faker', async ({ page }) => {
  await freshCollection(page);
  await newRequest(page);
  await page.getByRole('textbox', { name: 'URL' }).click();
  await page.keyboard.type('{{ $faker.person.f');
  const list = page.locator('.cm-tooltip-autocomplete');
  await expect(list).toContainText('person.firstName');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('textbox', { name: 'URL' })).toHaveText(
    '{{ $faker.person.firstName }}',
  );
});
