<!-- The MCP server for agents (ADR-014): on/off, port, token, allowed tools,
     client configs and the log of agent calls. -->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { McpGroup } from '$lib/api/generated/McpGroup';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import { capabilities } from '$lib/platform/capabilities.svelte';
  import { clientConfig, type McpClient, type McpStore } from '$lib/stores/mcp.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import Input from '$lib/ui/Input.svelte';
  import Switch from '$lib/ui/Switch.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import Field from '../Field.svelte';

  let {
    mcp,
    multiUser = false,
    origin = location.origin,
  }: {
    mcp: McpStore;
    /** A multi-user server: agents use personal API tokens. */
    multiUser?: boolean;
    origin?: string;
  } = $props();

  // svelte-ignore state_referenced_locally
  let port = $state(String(mcp.status?.config.port ?? 7655));
  let open = $state<McpGroup[]>([]);
  let client = $state<McpClient>('claude');
  const CLIENTS: McpClient[] = ['claude', 'cursor', 'vscode'];

  const desktop = $derived(capabilities.platform === 'desktop');
  const url = $derived(mcp.endpointUrl(origin));
  const shownToken = $derived(
    multiUser ? t('mcp.apiToken') : (mcp.token ?? t('mcp.tokenPlaceholder')),
  );
  const snippet = $derived(url ? clientConfig(client, url, shownToken) : '');

  $effect(() => {
    const saved = mcp.status?.config.port;
    if (saved !== undefined) port = String(saved);
  });

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async function savePort() {
    if (String(mcp.status?.config.port) === port.trim()) return;
    await run(async () => {
      if (!(await mcp.setPort(Number(port.trim())))) toasts.error(t('mcp.portInvalid'));
    });
  }

  async function regenerate() {
    const ok = await dialogs.confirm({
      title: t('mcp.regenerateTitle'),
      message: t('mcp.regenerateMessage'),
      confirmLabel: t('mcp.regenerate'),
      danger: true,
    });
    if (ok) await run(() => mcp.regenerate());
  }

  async function copy(text: string) {
    await navigator.clipboard.writeText(text);
    toasts.show({ message: t('mcp.copied'), kind: 'success' });
  }

  function toggleOpen(group: McpGroup) {
    open = open.includes(group) ? open.filter((g) => g !== group) : [...open, group];
  }

  const time = (ms: number) => new Date(ms).toLocaleTimeString();
  // i18n-dynamic: mcp.groups.
  // i18n-dynamic: mcp.client.

  onMount(() => void run(() => mcp.load()));
</script>

<p class="intro">{t('mcp.intro')}</p>

<Field label={t('mcp.enable')} hint={t('mcp.enableHint')}>
  <Switch
    label={t('mcp.enable')}
    checked={mcp.enabled}
    onchange={(on) => void run(() => mcp.setEnabled(on))}
  />
</Field>

{#if desktop}
  <Field label={t('mcp.port')} hint={t('mcp.portHint')}>
    <Input
      bind:value={port}
      inputmode="numeric"
      aria-label={t('mcp.port')}
      onblur={() => void savePort()}
      onkeydown={(e: KeyboardEvent) => {
        if (e.key === 'Enter') void savePort();
      }}
    />
  </Field>
{/if}

<Field label={t('mcp.endpoint')}>
  {#if mcp.enabled && url}
    <code class="mono value">{url}</code>
    <IconButton icon="copy" size={14} label={t('mcp.copy')} onclick={() => void copy(url)} />
  {:else if mcp.enabled && mcp.status?.listen.error}
    <span class="error" role="alert">{mcp.status.listen.error}</span>
  {:else}
    <span class="muted">{t('mcp.endpointOff')}</span>
  {/if}
</Field>

<Field label={t('mcp.token')} hint={multiUser ? t('mcp.apiTokenHint') : t('mcp.tokenHint')}>
  {#if multiUser}
    <span class="muted">—</span>
  {:else if mcp.token}
    <div class="stack">
      <div class="row">
        <code class="mono value">{mcp.token}</code>
        <IconButton
          icon="copy"
          size={14}
          label={t('mcp.copy')}
          onclick={() => void copy(mcp.token!)}
        />
        <Button size="sm" variant="ghost" onclick={() => mcp.hideToken()}>{t('mcp.hide')}</Button>
      </div>
      <span class="hint">{t('mcp.tokenOnce')}</span>
    </div>
  {:else if mcp.status?.hasToken}
    <span class="muted">{t('mcp.tokenSet')}</span>
    <Button size="sm" onclick={() => void regenerate()}>{t('mcp.regenerate')}</Button>
  {:else}
    <span class="muted">{t('mcp.tokenNone')}</span>
  {/if}
</Field>

<section class="block">
  <div class="block-head">
    <div>
      <h3>{t('mcp.tools')}</h3>
      <span class="hint">{t('mcp.toolsHint')}</span>
    </div>
    <div class="row">
      <Button size="sm" variant="ghost" onclick={() => void run(() => mcp.preset('readOnly'))}
        >{t('mcp.presetReadOnly')}</Button
      >
      <Button size="sm" variant="ghost" onclick={() => void run(() => mcp.preset('all'))}
        >{t('mcp.presetAll')}</Button
      >
    </div>
  </div>
  <ul class="groups">
    {#each mcp.groups as g (g.group)}
      {@const label = t(`mcp.groups.${g.group}`)}
      <li>
        <div class="group">
          <IconButton
            icon={open.includes(g.group) ? 'arrow-down' : 'arrow-up'}
            size={12}
            label={t('mcp.showTools', { group: label })}
            pressed={open.includes(g.group)}
            onclick={() => toggleOpen(g.group)}
          />
          <Checkbox
            {label}
            showLabel
            checked={g.enabled === g.total}
            onchange={(on) => void run(() => mcp.setGroup(g.group, on))}
          />
          <span class="count">{t('mcp.groupCount', { enabled: g.enabled, total: g.total })}</span>
        </div>
        {#if open.includes(g.group)}
          <ul class="tools">
            {#each g.tools as tool (tool.name)}
              <li>
                <Checkbox
                  label={tool.name}
                  showLabel
                  checked={tool.enabled}
                  onchange={(on) => void run(() => mcp.setTool(tool.name, on))}
                />
              </li>
            {/each}
          </ul>
        {/if}
      </li>
    {/each}
  </ul>
</section>

{#if url}
  <section class="block">
    <div class="block-head">
      <h3>{t('mcp.clients')}</h3>
      <div class="row" role="group" aria-label={t('mcp.clients')}>
        {#each CLIENTS as c (c)}
          <Button
            size="sm"
            variant={client === c ? 'primary' : 'ghost'}
            aria-pressed={client === c}
            onclick={() => (client = c)}>{t(`mcp.client.${c}`)}</Button
          >
        {/each}
      </div>
    </div>
    <div class="snippet">
      <pre class="mono">{snippet}</pre>
      <IconButton icon="copy" size={14} label={t('mcp.copy')} onclick={() => void copy(snippet)} />
    </div>
  </section>
{/if}

<section class="block">
  <h3>{t('mcp.log')}</h3>
  {#if mcp.log.length === 0}
    <span class="muted">{t('mcp.logEmpty')}</span>
  {:else}
    <table class="log">
      <thead>
        <tr>
          <th>{t('mcp.logTime')}</th>
          <th>{t('mcp.logTool')}</th>
          <th>{t('mcp.logArgs')}</th>
          <th>{t('mcp.logResult')}</th>
        </tr>
      </thead>
      <tbody>
        {#each mcp.log as call, i (i)}
          <tr>
            <td>{time(call.at)}</td>
            <td class="mono">{call.tool}</td>
            <td class="mono args" title={call.arguments}>{call.arguments}</td>
            <td class:failed={!call.ok} title={call.error ?? ''}
              >{call.ok ? t('mcp.logOk') : t('mcp.logFailed')} · {t('mcp.logDuration', {
                ms: call.durationMs,
              })}</td
            >
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</section>

<style>
  .intro {
    margin: 0 0 var(--space-2);
    color: var(--c-fg-muted);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .muted,
  .hint,
  .count {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
  }
  .hint {
    font-size: var(--font-size-xs);
  }
  .error {
    color: var(--c-danger);
  }
  .block {
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--c-border);
  }
  .block-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-bottom: var(--space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--font-size);
  }
  .groups,
  .tools {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 4px 0;
  }
  .tools {
    padding-left: 44px;
    font-family: var(--font-mono);
  }
  .tools li {
    padding: 2px 0;
  }
  .snippet {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
  }
  pre {
    flex: 1;
    min-width: 0;
    margin: 0;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }
  .log {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-sm);
    table-layout: fixed;
  }
  .log th {
    text-align: left;
    color: var(--c-fg-subtle);
    font-weight: normal;
  }
  .log td,
  .log th {
    padding: 4px var(--space-2) 4px 0;
    border-bottom: 1px solid var(--c-border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .log th:nth-child(1) {
    width: 80px;
  }
  .log th:nth-child(2) {
    width: 150px;
  }
  .log th:nth-child(4) {
    width: 120px;
  }
  .failed {
    color: var(--c-danger);
  }
</style>
