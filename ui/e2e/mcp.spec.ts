import { expect, test, type Page } from '@playwright/test';

test.use({ locale: 'en-US' });

interface RpcResponse {
  result?: {
    isError?: boolean;
    structuredContent?: Record<string, unknown>;
    serverInfo?: { name: string };
  };
  error?: { code: number };
}

async function openMcpSettings(page: Page) {
  await page.getByRole('contentinfo').getByRole('button', { name: 'Settings' }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings' });
  await dialog.getByRole('button', { name: 'MCP', exact: true }).click();
  return dialog;
}

test('an agent works through MCP and the app shows it', async ({ page, baseURL }) => {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Workspace' })).toBeVisible();
  const dialog = await openMcpSettings(page);
  await expect(dialog.getByText('Not running')).toBeVisible();

  await dialog.getByRole('switch', { name: 'MCP server' }).click();
  const endpoint = `${baseURL}/mcp`;
  await expect(dialog.getByText(endpoint, { exact: true })).toBeVisible();
  // The token is shown once, right after enabling.
  const token = (await dialog.locator('code').nth(1).textContent())!.trim();
  expect(token).toMatch(/^[0-9a-f]{64}$/);

  const rpc = async (method: string, params: object = {}) => {
    const res = await page.request.post(endpoint, {
      headers: { Authorization: `Bearer ${token}` },
      data: { jsonrpc: '2.0', id: 1, method, params },
    });
    expect(res.status()).toBe(200);
    return (await res.json()) as RpcResponse;
  };
  const tool = async (name: string, args: object) => {
    const body = await rpc('tools/call', { name, arguments: args });
    expect(body.result?.isError, JSON.stringify(body)).toBe(false);
    return body.result?.structuredContent ?? {};
  };

  const init = await rpc('initialize', { protocolVersion: '2025-06-18' });
  expect(init.result?.serverInfo?.name).toBe('reqly');

  // Editing is off by default: the agent may read and send, not create.
  const denied = await rpc('tools/call', { name: 'create_request', arguments: {} });
  expect(denied.error?.code).toBe(-32602);
  await dialog.getByRole('checkbox', { name: 'Edit requests' }).click();
  await expect(dialog.getByText('7 of 7')).toBeVisible();

  const { collections } = (await tool('list_collections', {})) as {
    collections: { id: string; name: string }[];
  };
  const col = collections.find((c) => c.name === 'My Collection')!;
  const name = `Agent ${Date.now().toString(36)}`;
  await tool('create_request', {
    collectionId: col.id,
    name,
    method: 'POST',
    url: `${baseURL}/healthz`,
  });

  // The log in the settings shows the calls as they happen.
  await expect(dialog.getByRole('cell', { name: 'create_request' }).first()).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('treeitem', { name: new RegExp(name) })).toBeVisible();
  await expect(page.getByRole('button', { name: 'MCP server is on' })).toBeVisible();

  // Turn it off again: the endpoint disappears.
  await page.getByRole('button', { name: 'MCP server is on' }).click();
  await page
    .getByRole('dialog', { name: 'Settings' })
    .getByRole('switch', { name: 'MCP server' })
    .click();
  await expect(
    page.getByRole('dialog', { name: 'Settings' }).getByText('Not running'),
  ).toBeVisible();
  const res = await page.request.post(endpoint, {
    headers: { Authorization: `Bearer ${token}` },
    data: { jsonrpc: '2.0', id: 1, method: 'ping' },
  });
  expect(res.status()).toBe(404);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'MCP server is on' })).toBeHidden();

  // Leave the tree as it was.
  await page.getByRole('treeitem', { name: new RegExp(name) }).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Delete' }).click();
  await page.getByRole('alertdialog').getByRole('button', { name: 'Delete' }).click();
});
