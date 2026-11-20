import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { expect, test } from '@playwright/test';

test.use({ locale: 'en-US' });

interface ChatBody {
  messages: { role: string; content: string | null }[];
}

/**
 * A fake OpenAI-compatible provider: lists the collections, then creates a
 * request in the first one (the user must allow it), then answers.
 */
function fakeProvider(): Promise<{ server: Server; url: string }> {
  const sse = (chunks: object[]) =>
    chunks.map((c) => `data: ${JSON.stringify(c)}\n\n`).join('') + 'data: [DONE]\n\n';
  const choice = (delta: object, finish: string | null = null) => ({
    choices: [{ index: 0, delta, finish_reason: finish }],
  });
  const call = (name: string, args: object) =>
    sse([
      choice({
        role: 'assistant',
        tool_calls: [
          {
            index: 0,
            id: `call_${name}`,
            type: 'function',
            function: { name, arguments: JSON.stringify(args) },
          },
        ],
      }),
      choice({}, 'tool_calls'),
    ]);
  const server = createServer((req, res) => {
    let raw = '';
    req.on('data', (c: Buffer) => (raw += c.toString()));
    req.on('end', () => {
      if (req.url === '/v1/models') {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ data: [{ id: 'fake-small' }, { id: 'fake-large' }] }));
        return;
      }
      const body = JSON.parse(raw) as ChatBody;
      const tools = body.messages.filter((m) => m.role === 'tool');
      res.writeHead(200, { 'content-type': 'text/event-stream' });
      if (tools.length === 0) {
        res.end(call('list_collections', {}));
      } else if (tools.length === 1) {
        const { collections } = JSON.parse(tools[0]!.content!) as {
          collections: { id: string }[];
        };
        res.end(
          call('create_request', {
            collectionId: collections[0]!.id,
            name: 'Made by AI',
            method: 'GET',
            url: 'https://example.com/ai',
          }),
        );
      } else {
        res.end(
          sse([
            choice({ role: 'assistant', content: 'Created ' }),
            choice({ content: '**Made by AI**.' }, 'stop'),
          ]),
        );
      }
    });
  });
  return new Promise((resolve) =>
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address() as AddressInfo;
      resolve({ server, url: `http://127.0.0.1:${port}/v1` });
    }),
  );
}

test('the AI chat creates a request after the user allows it', async ({ page }) => {
  const { server, url } = await fakeProvider();
  try {
    await page.goto('/');
    await expect(page.getByRole('combobox', { name: 'Workspace' })).toBeVisible();
    await expect(
      page.getByRole('contentinfo').getByRole('button', { name: 'AI chat' }),
    ).toHaveCount(0);

    // Settings → AI: turn it on, add the fake provider, pick a model.
    await page.getByRole('contentinfo').getByRole('button', { name: 'Settings' }).click();
    const dialog = page.getByRole('dialog', { name: 'Settings' });
    await dialog.getByRole('button', { name: 'AI', exact: true }).click();
    await dialog.getByRole('switch', { name: 'AI chat' }).click();
    await dialog.getByRole('button', { name: 'Add provider' }).click();
    await page.getByRole('menuitem', { name: 'OpenAI-compatible' }).click();
    const card = dialog.getByRole('article', { name: 'OpenAI-compatible' });
    const baseUrl = card.getByRole('textbox', { name: 'Base URL' });
    await baseUrl.fill(url);
    await baseUrl.blur();
    await card.getByRole('button', { name: 'Check connection' }).click();
    await expect(page.getByText('Connected: 2 models')).toBeVisible();
    const model = dialog.getByRole('combobox', { name: 'Model' });
    await model.click();
    await page.getByRole('option', { name: 'fake-small' }).click();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();

    // The panel: ask, allow the change, read the answer.
    await page.getByRole('contentinfo').getByRole('button', { name: 'AI chat' }).click();
    const panel = page.getByRole('region', { name: 'AI chat' });
    const input = panel.getByRole('textbox', { name: /Ask about your requests/ });
    await input.fill('Add a request for the AI endpoint');
    await input.press('Enter');
    await expect(panel.getByText('Allow “create_request”?')).toBeVisible();
    await expect(page.getByRole('treeitem', { name: /Made by AI/ })).toHaveCount(0);
    await panel.getByRole('button', { name: 'Allow', exact: true }).click();
    await expect(panel.locator('strong', { hasText: 'Made by AI' })).toBeVisible();
    await expect(page.getByRole('treeitem', { name: /Made by AI/ })).toBeVisible();
    // The answer can be selected and copied from the context menu.
    const answer = panel.locator('strong', { hasText: 'Made by AI' });
    await expect(answer).toHaveCSS('user-select', 'text');
    await answer.click({ button: 'right' });
    await expect(page.getByRole('menuitem', { name: 'Select all' })).toBeVisible();
    await page.keyboard.press('Escape');
    // The chat got its title from the question.
    await expect(panel.getByRole('button', { name: 'Chats' })).toContainText(
      'Add a request for the AI endpoint',
    );

    // Turn the chat off again for the other tests.
    await page.getByRole('contentinfo').getByRole('button', { name: 'Settings' }).click();
    await dialog.getByRole('button', { name: 'AI', exact: true }).click();
    await dialog.getByRole('switch', { name: 'AI chat' }).click();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('region', { name: 'AI chat' })).toBeHidden();
  } finally {
    server.close();
  }
});
