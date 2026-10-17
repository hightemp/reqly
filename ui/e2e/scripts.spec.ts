import { expect, test, type Page } from '@playwright/test';

test.use({ locale: 'en-US' });

async function typeInto(page: Page, label: string, text: string) {
  const editor = page.getByRole('textbox', { name: label, exact: true });
  await editor.click();
  await page.keyboard.press('Control+A');
  await page.keyboard.press('Delete');
  // insertText keeps auto-closing brackets out of the way.
  await page.keyboard.insertText(text);
}

test('pre-request and after-response scripts', async ({ page, baseURL }) => {
  test.setTimeout(60_000);
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toBeVisible();
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: 'New request' }).click();
  // The new request's editor may replace the previous tab's one: retry until it sticks.
  const url = page.getByRole('textbox', { name: 'URL' });
  await expect(async () => {
    await url.fill(`${baseURL}/healthz?run={{ run }}`);
    await expect(url).toHaveText(`${baseURL}/healthz?run={{ run }}`, { timeout: 1000 });
  }).toPass();

  await page.getByRole('tab', { name: 'Scripts' }).click();
  await typeInto(
    page,
    'Pre-request',
    "pm.variables.set('run', 'scripted');\nconsole.log('pre', rq.info.requestName);",
  );
  await page.getByRole('tab', { name: 'After-response' }).click();
  await typeInto(
    page,
    'After-response',
    "rq.test('status is 200', () => rq.response.to.have.status(200));\nrq.test('fails on purpose', () => rq.expect(1).to.equal(2));\nconsole.warn('done', rq.response.json().status);",
  );

  await page.waitForTimeout(800);
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();

  await page.getByRole('tab', { name: /^Tests/ }).click();
  await expect(page.getByText('1 passed')).toBeVisible();
  await expect(page.getByText('1 failed')).toBeVisible();
  await expect(page.getByText('expected 1 to equal 2')).toBeVisible();

  await page.getByRole('tab', { name: /^Console/ }).click();
  const consoleLog = page.getByRole('log', { name: 'Console' });
  await expect(consoleLog).toContainText('pre New Request');
  await expect(consoleLog).toContainText('done ok');

  // The runtime variable reached the URL and is listed in the environments.
  await page.getByRole('tab', { name: 'Timeline' }).click();
  await expect(page.getByText(/GET \/healthz\?run=scripted HTTP/)).toBeVisible();
  await page.getByRole('button', { name: /^Environment:/ }).click();
  await page.getByRole('menuitem', { name: /Manage environments/ }).click();
  await page.getByRole('option', { name: 'Runtime (scripts)' }).click();
  await expect(page.getByRole('dialog').getByRole('row', { name: /run scripted/ })).toBeVisible();
  await page.getByRole('dialog').getByRole('button', { name: 'Clear', exact: true }).click();
  await expect(page.getByText('No runtime variables')).toBeVisible();
  await page.getByRole('dialog').press('Escape');
  await expect(page.getByRole('dialog')).toBeHidden();

  // Clean up tabs for the next specs.
  await page
    .getByRole('tablist', { name: 'Open requests' })
    .getByRole('tab')
    .first()
    .click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Close all' }).click();
  await page.waitForTimeout(800);
});
