<!-- Supported import formats: where the files come from, what is imported, examples. -->
<script lang="ts">
  import { t } from '$lib/i18n';
  import Button from '$lib/ui/Button.svelte';
  import Modal from '$lib/ui/overlay/Modal.svelte';

  let { onclose }: { onclose: () => void } = $props();

  interface Item {
    name: string;
    files: string;
    text: string;
    example?: string;
  }

  const items: Item[] = $derived([
    { name: 'Reqly', files: '*.reqly.json, *.reqly.yaml', text: t('import.help.reqly') },
    {
      name: 'Insomnia v3 / v4 / v5',
      files: 'JSON, YAML',
      text: t('import.help.insomnia'),
    },
    {
      name: 'Postman v2.0 / v2.1',
      files: '*.postman_collection.json, *.postman_environment.json',
      text: t('import.help.postman'),
    },
    {
      name: 'OpenAPI 3.0 / 3.1, Swagger 2.0',
      files: 'JSON, YAML',
      text: t('import.help.openapi'),
    },
    { name: 'HAR 1.2', files: '*.har', text: t('import.help.har') },
    {
      name: 'cURL',
      files: '*.sh, *.txt',
      text: t('import.help.curl'),
      example:
        'curl -X POST https://api.example.com/items \\\n  -H \'Content-Type: application/json\' \\\n  -d \'{"name": "Ada"}\'\n\ncurl https://api.example.com/items?page=2',
    },
    {
      name: 'HTTP (.http / .rest)',
      files: '*.http, *.rest',
      text: t('import.help.http'),
      example:
        '@host = https://api.example.com\n\n### List items\nGET {{host}}/items?page=1\nAccept: application/json\n\n### Create an item\nPOST {{host}}/items\nContent-Type: application/json\n\n{"name": "Ada"}',
    },
    {
      name: t('import.help.rawName'),
      files: '—',
      text: t('import.help.raw'),
      example:
        'POST /items HTTP/1.1\nHost: api.example.com\nContent-Type: application/json\n\n{"name": "Ada"}',
    },
    { name: 'Bruno', files: '*.bru, zip', text: t('import.help.bruno') },
  ]);
</script>

<Modal title={t('import.help.title')} width="720px" {onclose}>
  <p class="intro">{t('import.help.intro')}</p>
  <ul class="formats" aria-label={t('import.help.title')}>
    {#each items as item (item.name)}
      <li>
        <div class="head">
          <h3>{item.name}</h3>
          <span class="files">{item.files}</span>
        </div>
        <p>{item.text}</p>
        {#if item.example}
          <pre aria-label={t('import.help.example', { name: item.name })}>{item.example}</pre>
        {/if}
      </li>
    {/each}
  </ul>
  {#snippet footer()}
    <Button variant="primary" onclick={onclose}>{t('common.close')}</Button>
  {/snippet}
</Modal>

<style>
  .intro {
    margin: 0 0 var(--space-3);
    color: var(--c-fg-muted);
    font-size: var(--font-size-sm);
  }
  .formats {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 60vh;
    overflow: auto;
  }
  .formats li {
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--c-border);
  }
  .formats li:last-child {
    border-bottom: none;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
  }
  h3 {
    margin: 0;
    font-size: var(--font-size-sm);
  }
  .files {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--c-fg-muted);
  }
  p {
    margin: var(--space-1) 0 0;
    font-size: var(--font-size-sm);
  }
  pre {
    margin: var(--space-2) 0 0;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--c-bg-sunken);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
