<script lang="ts">
  import CodeEditor from './code/CodeEditor.svelte';
  import KeyValueEditor from './kv/KeyValueEditor.svelte';
  import type { KvRow } from './kv/bulk';
  import Tabs from './Tabs.svelte';

  let { kind, onchange }: { kind: string; onchange?: (v: never) => void } = $props();
  let tab = $state<'params' | 'body' | 'headers'>('params');
  let rows = $state<KvRow[]>([
    { id: '1', enabled: true, name: 'Accept', value: '*/*', description: '', kind: 'text' },
  ]);
  let code = $state('{"a":1}');
  let editorRef: CodeEditor | undefined = $state();
  export const editor = () => editorRef;
</script>

{#if kind === 'tabs'}
  <Tabs
    label="Request"
    bind:value={tab}
    tabs={[
      { id: 'params', label: 'Params' },
      { id: 'body', label: 'Body' },
      { id: 'headers', label: 'Headers', count: 3 },
    ]}
  />
  <span data-testid="tab">{tab}</span>
{:else if kind === 'kv'}
  <KeyValueEditor
    label="Headers"
    bind:rows
    inherited={[{ name: 'X-Team', value: 'core', source: 'api folder' }]}
    onchange={onchange as never}
  />
{:else if kind === 'code' || kind === 'code-readonly'}
  <div style="height: 200px">
    <CodeEditor
      bind:this={editorRef}
      label="Body"
      language="json"
      bind:value={code}
      readonly={kind === 'code-readonly'}
      onchange={onchange as never}
    />
  </div>
  <button data-testid="set-external" onclick={() => (code = '[]')}>.</button>
{/if}
