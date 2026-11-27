import { expect, test, type Page } from '@playwright/test';

test.use({ locale: 'en-US', colorScheme: 'dark' });

test('create a request, send it and see the response', async ({ page, baseURL }) => {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toHaveText(/My Collection/);

  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const tab = page.getByRole('tab', { name: /New Request/ });
  await expect(tab).toBeVisible();

  const url = page.getByRole('textbox', { name: 'URL' });
  await url.fill(`${baseURL}/healthz?probe=1`);
  await expect(page.getByRole('tab', { name: /Params/ })).toContainText('1');
  await page.getByRole('button', { name: 'Send', exact: true }).click();

  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('"status": "ok"');

  await page.getByRole('tab', { name: 'Timeline' }).click();
  await expect(page.getByText(/GET \/healthz\?probe=1 HTTP/)).toBeVisible();

  // Autosave + restored tabs and history after reload.
  await page.waitForTimeout(800);
  await page.reload();
  await expect(page.getByRole('tab', { name: /New Request/ })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'URL' })).toHaveText(`${baseURL}/healthz?probe=1`);
  await expect(page.getByText('200 OK')).toBeVisible();
});

/** Closes every open tab so the next request's editor is the only one. */
async function closeAllTabs(page: Page) {
  // Wait until the app (and restored tabs) are rendered.
  await expect(page.getByRole('combobox', { name: 'Workspace' })).toBeVisible();
  const tab = page.getByRole('tablist', { name: 'Open requests' }).getByRole('tab').first();
  if (await tab.isVisible().catch(() => false)) {
    await tab.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Close all' }).click();
  }
  await expect(page.getByRole('tablist', { name: 'Open requests' })).toBeHidden();
}

test('pasting a cURL command imports it', async ({ page, baseURL }) => {
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const url = page.getByRole('textbox', { name: 'URL' });
  await url.focus();
  await page.evaluate((cmd) => {
    const data = new DataTransfer();
    data.setData('text', cmd);
    document.activeElement!.dispatchEvent(
      new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, `curl -X POST '${baseURL}/api/v1/cmd/app.info' -H 'Content-Type: application/json' -d '{}'`);
  await page.getByRole('button', { name: 'Import' }).click();
  await expect(page.getByRole('combobox', { name: 'Method' })).toHaveValue('POST');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('"platform": "server"');
});

test('pasting HTTP request text imports it', async ({ page, baseURL }) => {
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const url = page.getByRole('textbox', { name: 'URL' });
  await url.focus();
  const host = new URL(baseURL!).host;
  await page.evaluate((text) => {
    const data = new DataTransfer();
    data.setData('text', text);
    document.activeElement!.dispatchEvent(
      new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, `POST /api/v1/cmd/app.info HTTP/1.1\nHost: ${host}\nContent-Type: application/json\n\n{}`);
  await page.getByRole('button', { name: 'Import' }).click();
  await expect(page.getByRole('combobox', { name: 'Method' })).toHaveValue('POST');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('"platform": "server"');
});

test('folders, rename and delete with undo', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New folder' }).click();
  const rename = page.getByRole('tree', { name: 'Collections' }).getByRole('textbox');
  await rename.fill('Users API');
  await rename.press('Enter');
  const folder = page.getByRole('treeitem', { name: /Users API/ });
  await expect(folder).toBeVisible();

  await folder.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'Delete' }).click();
  await expect(folder).toBeHidden();
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.getByRole('treeitem', { name: /Users API/ })).toBeVisible();
});

test('command palette opens requests and runs commands', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await page
    .getByRole('tab', { name: /New Request/ })
    .last()
    .click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Close all' }).click();
  await expect(page.getByRole('tablist', { name: 'Open requests' })).toBeHidden();

  await page.keyboard.press('Control+k');
  const input = page.getByRole('combobox', { name: 'Go to request or command' });
  await input.fill('new req');
  await input.press('Enter');
  await expect(page.getByRole('tab', { name: /New Request/ })).toHaveCount(1);

  await page.keyboard.press('Control+k');
  await page.getByRole('combobox', { name: 'Go to request or command' }).fill('>cookies');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog', { name: 'Cookies' })).toBeVisible();
});

test('template completion and hover tooltips are not clipped by the input', async ({ page }) => {
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const url = page.getByRole('textbox', { name: 'URL' });
  await url.click();
  await page.keyboard.type('https://x.test/{{$gu');
  const completion = page.locator('.cm-tooltip-autocomplete');
  await expect(completion).toBeVisible();
  // Rendered outside the input (WebKit positions tooltips inside the
  // editor otherwise, where the input's overflow cuts them off).
  expect(await completion.evaluate((el) => !!el.closest('.template-input'))).toBe(false);
  const input = (await page.locator('.template-input').first().boundingBox())!;
  const box = (await completion.boundingBox())!;
  // Below the text line, reaching well past the input.
  expect(box.y).toBeGreaterThan(input.y + input.height / 2);
  expect(box.y + box.height).toBeGreaterThan(input.y + input.height + 8);
  await page.keyboard.press('Escape');

  // Right after `{{` the list has the functions too.
  await page.keyboard.press('ControlOrMeta+A');
  await page.keyboard.type('https://x.test/{{ uu');
  await expect(completion.getByText('$uuid')).toBeVisible();
  await page.keyboard.press('Enter');
  await expect(url).toHaveText('https://x.test/{{ $uuid }}');
  await closeAllTabs(page);
});

test('many open tabs scroll with the wheel and the arrows', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await page.goto('/');
  await closeAllTabs(page);
  for (let i = 0; i < 12; i++) {
    await page.getByRole('button', { name: 'Create' }).click();
    await page.getByRole('menuitem', { name: 'New request' }).click();
  }
  const strip = page.getByRole('tablist', { name: 'Open requests' });
  await expect(strip.getByRole('tab')).toHaveCount(12);
  const bar = strip.locator('..');
  const left = bar.getByRole('button', { name: 'Scroll tabs left' });
  const right = bar.getByRole('button', { name: 'Scroll tabs right' });
  const scrollLeft = () => strip.evaluate((el) => el.scrollLeft);

  // The newest tab is active and scrolled into view: hidden tabs are on the left.
  await expect(left).toBeEnabled();
  await expect(right).toBeDisabled();

  // A vertical mouse wheel scrolls the strip sideways.
  const before = await scrollLeft();
  await strip.hover();
  await page.mouse.wheel(0, -300);
  await expect.poll(scrollLeft).toBeLessThan(before);
  await expect(right).toBeEnabled();

  // The arrows page through the tabs.
  while (await left.isEnabled()) {
    await left.click();
    await page.waitForTimeout(400); // smooth scrolling
  }
  await expect.poll(scrollLeft).toBe(0);
  await expect(left).toBeDisabled();
  await right.click();
  await expect.poll(scrollLeft).toBeGreaterThan(0);

  // No arrows once the tabs fit again.
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await expect(strip.getByRole('tab')).toHaveCount(1);
  await expect(left).toBeHidden();
  await closeAllTabs(page);
});

test('text has a context menu and the selection shows over the current line', async ({
  page,
  context,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await page.getByRole('tab', { name: 'Body' }).click();
  await page.getByRole('combobox', { name: 'Body type' }).click();
  await page.getByRole('option', { name: 'Plain text' }).click();
  const editor = page.getByRole('textbox', { name: 'Request body' });
  await editor.click();
  await page.keyboard.type('hello world');

  // Select all from the menu: the current line stops covering the selection.
  await editor.click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: 'Cut' })).toBeDisabled();
  await page.getByRole('menuitem', { name: 'Select all' }).click();
  const line = editor.locator('.cm-activeLine');
  await expect(line).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');

  // Copy, then paste at the end.
  await editor.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Copy' }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('hello world');
  // A right click in Chrome moves the selection: open the menu at the caret.
  await page.keyboard.press('ControlOrMeta+End');
  await editor.evaluate((el) => {
    const r = el.getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: r.x + 20,
        clientY: r.y + 10,
      }),
    );
  });
  await page.getByRole('menuitem', { name: 'Paste' }).click();
  await expect(editor).toHaveText('hello worldhello world');

  await closeAllTabs(page);
});

test('section tabs scroll in a narrow panel', async ({ page }) => {
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await page.setViewportSize({ width: 800, height: 700 });
  const sections = page.getByRole('tablist', { name: 'Request sections' });
  const bar = sections.locator('..');
  const right = bar.getByRole('button', { name: 'Scroll tabs right' });
  await expect(right).toBeEnabled();
  const scrollLeft = () => sections.evaluate((el) => el.scrollLeft);
  await sections.hover();
  await page.mouse.wheel(0, 200);
  await expect.poll(scrollLeft).toBeGreaterThan(0);
  await expect(bar.getByRole('button', { name: 'Scroll tabs left' })).toBeEnabled();
  await page.setViewportSize({ width: 1280, height: 720 });
  await closeAllTabs(page);
});

test('response tabs copy selected text with Ctrl+C and the context menu', async ({
  page,
  context,
  baseURL,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const url = page.getByRole('textbox', { name: 'URL' });
  await url.fill(`${baseURL}/healthz`);
  await expect(url).toHaveText(`${baseURL}/healthz`);
  // The tree had the focus last: its own Ctrl+C must not take over.
  const tree = page.getByRole('tree', { name: 'Collections' });
  await tree.getByRole('treeitem').first().focus();
  await page.keyboard.press('ControlOrMeta+Enter');
  await expect(page.getByText('200 OK')).toBeVisible();
  const clipboard = () => page.evaluate(() => navigator.clipboard.readText());
  const clear = () => page.evaluate(() => navigator.clipboard.writeText(''));

  // Body (a read-only editor).
  const body = page.locator('.response .cm-content');
  await body.click();
  await page.keyboard.press('ControlOrMeta+A');
  await page.keyboard.press('ControlOrMeta+C');
  expect(await clipboard()).toContain('"status": "ok"');

  // Headers: a selection made with the mouse.
  await clear();
  await page
    .getByRole('tab', { name: /Headers/ })
    .last()
    .click();
  await tree.getByRole('treeitem').first().focus();
  const cell = page.locator('.response table td').first();
  const box = (await cell.boundingBox())!;
  await page.mouse.move(box.x + 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  const word = await page.evaluate(() => window.getSelection()!.toString());
  expect(word.trim()).not.toBe('');
  await page.keyboard.press('ControlOrMeta+C');
  expect(await clipboard()).toBe(word);

  // The menu over a header row copies its parts.
  await cell.click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: /^Copy Ctrl/ })).toBeEnabled();
  await page.getByRole('menuitem', { name: 'Copy row' }).click();
  expect(await clipboard()).toMatch(/^[\w-]+: .+/);

  // Timeline.
  await clear();
  await page.getByRole('tab', { name: 'Timeline' }).click();
  const line = (await page.locator('.response .timeline .line').first().boundingBox())!;
  await page.mouse.move(line.x + 2, line.y + line.height / 2);
  await page.mouse.down();
  await page.mouse.move(line.x + line.width - 2, line.y + line.height / 2, { steps: 5 });
  await page.mouse.up();
  await page.keyboard.press('ControlOrMeta+C');
  expect(await clipboard()).toContain('Trying 127.0.0.1');

  await closeAllTabs(page);
});

test('the description has a text menu when editing and in the preview', async ({
  page,
  context,
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  await page.getByRole('tab', { name: 'Docs' }).click();
  const area = page.getByRole('textbox', { name: 'Description' });
  await area.fill('Some **notes**');
  await area.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Select all' }).click();
  // A right click in Chrome moves the caret: open the menu over the selection.
  await area.evaluate((el) => {
    const r = el.getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: r.x + 20,
        clientY: r.y + 10,
      }),
    );
  });
  await page.getByRole('menuitem', { name: /^Copy Ctrl/ }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Some **notes**');

  await page.getByRole('tab', { name: 'Preview' }).click();
  await page.locator('.docs .preview strong').click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: 'Select all' })).toBeVisible();
  await expect(page.getByRole('menuitem', { name: 'Paste' })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await closeAllTabs(page);
});

test('copy the path, the ID and a block for AI of a request', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/');
  await closeAllTabs(page);
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  const url = page.getByRole('textbox', { name: 'URL' });
  await url.fill('https://ref.test/items');
  const clipboard = () => page.evaluate(() => navigator.clipboard.readText());
  const collection = (await page.getByRole('combobox', { name: 'Collection' }).innerText()).trim();

  const tab = page.getByRole('tablist', { name: 'Open requests' }).getByRole('tab').first();
  const copyRef = async (item: string) => {
    await tab.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Copy path / ID' }).hover();
    await page.getByRole('menuitem', { name: item, exact: true }).click();
  };
  await copyRef('Copy path');
  await expect.poll(clipboard).toBe(`${collection} / New Request`);
  await copyRef('Copy ID');
  await expect.poll(clipboard).toMatch(/^[0-9a-f-]{36}$/);
  const id = await clipboard();

  // The tree rows have the same submenu.
  await page.getByRole('treeitem').first().click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Copy path / ID' }).hover();
  await expect(page.getByRole('menuitem', { name: 'Copy for AI' })).toBeVisible();
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');

  // The unsaved URL is in the block too.
  await tab.click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Copy path / ID' }).hover();
  await page.getByRole('menuitem', { name: 'Copy for AI' }).click();
  await expect
    .poll(clipboard)
    .toBe(
      [
        'Reqly request "New Request"',
        `Path: ${collection} / New Request`,
        `ID: ${id}`,
        'GET https://ref.test/items',
        `Read it with the Reqly MCP tool get_request {"id": "${id}"}.`,
      ].join('\n'),
    );
  await closeAllTabs(page);
});
