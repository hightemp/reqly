import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { expect, test, type Page } from '@playwright/test';
import { buildSchema, graphql } from 'graphql';
import { WebSocketServer } from 'ws';

test.use({ locale: 'en-US' });

// Local fixtures: SSE, a GraphQL endpoint (graphql-js) and a WebSocket echo.
let server: Server;
let wss: WebSocketServer;
let base = '';

const schema = buildSchema(`
  "Root queries"
  type Query { hello(name: String): String, user: User }
  "A user"
  type User { id: ID!, name: String }
`);
const root = {
  hello: ({ name }: { name?: string }) => `Hello, ${name ?? 'world'}!`,
  user: () => ({ id: '1', name: 'Ada' }),
};

test.beforeAll(async () => {
  server = createServer((req, res) => {
    const url = new URL(req.url ?? '/', 'http://x');
    if (url.pathname === '/events') {
      res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
      res.write('event: greeting\ndata: hello\n\n');
      res.write('id: 2\nevent: update\ndata: {"n":2}\n\n');
      res.write('data: plain\n\n');
      return; // keep open
    }
    if (url.pathname === '/graphql') {
      let body = '';
      req.on('data', (c) => (body += c));
      req.on('end', async () => {
        const { query, variables, operationName } = JSON.parse(body || '{}');
        const result = await graphql({
          schema,
          source: query,
          rootValue: root,
          variableValues: variables,
          operationName,
        });
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify(result));
      });
      return;
    }
    res.writeHead(404).end();
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  base = `127.0.0.1:${(server.address() as AddressInfo).port}`;
  wss = new WebSocketServer({ server, path: '/ws' });
  wss.on('connection', (socket) =>
    socket.on('message', (data, binary) => socket.send(binary ? data : `echo: ${data}`)),
  );
});

test.afterAll(() => {
  wss.close();
  server.close();
});

async function create(page: Page, item: string) {
  await page.goto('/');
  await expect(page.getByRole('combobox', { name: 'Collection' })).toBeVisible();
  await page.getByRole('button', { name: 'Create' }).click();
  await page.getByRole('menuitem', { name: item }).click();
}

async function closeTabs(page: Page) {
  await page
    .getByRole('tablist', { name: 'Open requests' })
    .getByRole('tab')
    .first()
    .click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Close all' }).click();
  await expect(page.getByRole('tablist', { name: 'Open requests' })).toBeHidden();
  await page.waitForTimeout(800);
}

test('server-sent events', async ({ page }) => {
  await create(page, 'New event stream (SSE)');
  await expect(page.getByRole('treeitem', { name: /SSE/ }).first()).toBeVisible();
  await page.getByRole('textbox', { name: 'URL' }).fill(`http://${base}/events`);
  await page.getByRole('button', { name: 'Connect' }).click();
  const log = page.getByRole('listbox', { name: 'Messages' });
  await expect(log.getByRole('option')).toHaveCount(4); // open + 3 events
  await expect(log).toContainText('hello');
  await expect(page.getByText('Connected', { exact: true })).toBeVisible();

  // Filter by event type, open a JSON message.
  await page.getByRole('combobox', { name: 'Event type' }).click();
  await page.getByRole('option', { name: 'update', exact: true }).click();
  await expect(log.getByRole('option')).toHaveCount(1);
  await log.getByRole('option').first().click();
  await expect(page.locator('.cm-content').last()).toContainText('"n": 2');

  await page.getByRole('button', { name: 'Disconnect' }).first().click();
  await expect(page.getByText('Disconnected', { exact: true })).toBeVisible();
  await closeTabs(page);
});

test('websocket', async ({ page }) => {
  await create(page, 'New WebSocket request');
  await page.getByRole('textbox', { name: 'URL' }).fill(`ws://${base}/ws`);
  await page.getByRole('button', { name: 'Connect' }).click();
  await expect(page.getByText('Connected', { exact: true })).toBeVisible();
  const composer = page.getByRole('textbox', { name: 'Message', exact: true });
  await composer.click();
  await page.keyboard.type('ping');
  await page.keyboard.press('Control+Enter');
  const log = page.getByRole('listbox', { name: 'Messages' });
  await expect(log).toContainText('echo: ping');

  // Saved messages.
  await page.getByRole('button', { name: 'Save message' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'OK' }).click();
  await expect(page.getByRole('heading', { name: 'Saved messages' })).toBeVisible();

  await page.getByRole('button', { name: 'Disconnect' }).click();
  await expect(page.getByText('Disconnected', { exact: true })).toBeVisible();
  await closeTabs(page);
});

test('graphql query, schema and docs', async ({ page }) => {
  await create(page, 'New GraphQL request');
  await page.getByRole('textbox', { name: 'URL' }).fill(`http://${base}/graphql`);
  const query = page.getByRole('textbox', { name: 'Query' });
  await query.click();
  await page.keyboard.press('Control+A');
  await page.keyboard.type('query Q($n: String) { hello(name: $n) }');
  const variables = page.getByRole('textbox', { name: 'Variables' });
  await variables.click();
  await page.keyboard.type('{"n": "Reqly"');
  await page.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByText('200 OK')).toBeVisible();
  await expect(page.locator('.cm-content').last()).toContainText('Hello, Reqly!');

  // Introspection and the docs explorer.
  await page.getByRole('button', { name: 'No schema' }).click();
  await page.getByRole('menuitem', { name: 'Fetch schema (introspection)' }).click();
  await expect(page.getByRole('button', { name: /^Schema: / })).toBeVisible();
  await page.getByRole('button', { name: 'Docs', exact: true }).click();
  const docs = page.getByRole('complementary', { name: 'Docs' });
  await docs.getByRole('button', { name: 'Query' }).click();
  await expect(docs).toContainText('Root queries');
  await docs.getByRole('button', { name: 'User' }).click();
  await expect(docs).toContainText('A user');
  await closeTabs(page);
});
