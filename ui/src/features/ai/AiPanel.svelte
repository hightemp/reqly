<!-- The AI chat panel (ADR-015): chats of the workspace, messages with tool
     cards, the composer with attachments and the model of the chat. -->
<script lang="ts">
  import { tick } from 'svelte';
  import type { AiAttachmentRef } from '$lib/api/generated/AiAttachmentRef';
  import type { AiDecision } from '$lib/api/generated/AiDecision';
  import { errorMessage } from '$lib/errors';
  import { t } from '$lib/i18n';
  import type { Services } from '$lib/services.svelte';
  import Button from '$lib/ui/Button.svelte';
  import EmptyState from '$lib/ui/EmptyState.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import IconButton from '$lib/ui/IconButton.svelte';
  import { dialogs } from '$lib/ui/overlay/dialogs.svelte';
  import { menus, type MenuEntry } from '$lib/ui/overlay/menus.svelte';
  import { toasts } from '$lib/ui/overlay/toasts.svelte';
  import { tabInfo } from '../tabs/info.svelte';
  import AiMessage from './AiMessage.svelte';
  import ToolCard from './ToolCard.svelte';

  interface Props {
    services: Services;
    onclose?: () => void;
    onsettings: () => void;
  }

  let { services, onclose, onsettings }: Props = $props();
  const ai = $derived(services.ai);
  const chat = $derived(services.aiChat);

  interface Attachment extends AiAttachmentRef {
    label: string;
  }

  let text = $state('');
  let attachments = $state<Attachment[]>([]);
  let scroller: HTMLDivElement | null = $state(null);

  $effect(() => {
    const ws = services.workspace.activeWorkspaceId;
    if (ai.enabled) void chat.setWorkspace(ws).catch((e: unknown) => toasts.error(errorMessage(e)));
  });

  // Models of every provider, for the model menu (quietly: a provider may be down).
  $effect(() => {
    if (!ai.enabled) return;
    for (const p of ai.providers) {
      if (!ai.models[p.id]) void ai.loadModels(p.id).catch(() => {});
    }
  });

  // Keep the newest message in view while the user is at the bottom.
  let atBottom = true;
  $effect(() => {
    void chat.messages.length;
    void chat.messages.at(-1)?.content;
    if (atBottom)
      void tick().then(() => {
        if (scroller) scroller.scrollTop = scroller.scrollHeight;
      });
  });

  function onscroll() {
    if (!scroller) return;
    atBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
  }

  const ready = $derived(ai.providers.length > 0);
  const activeTitle = $derived(chat.active?.title || t('ai.untitled'));
  const modelLabel = $derived(chat.active?.model ?? ai.defaultModel?.model ?? t('ai.chooseModel'));
  const requestId = $derived(services.tabs.activeId);
  const responseId = $derived(
    requestId ? (services.responses.run(requestId).current?.id ?? null) : null,
  );

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  function chatsMenu(e: MouseEvent) {
    const entries: MenuEntry[] = [
      { id: 'new', label: t('ai.newChat'), onSelect: () => void run(() => chat.newChat()) },
      { separator: true },
      ...(chat.chats.length
        ? chat.chats.map((c) => ({
            id: c.id,
            label: c.title || t('ai.untitled'),
            checked: c.id === chat.activeId,
            onSelect: () => void run(() => chat.open(c.id)),
          }))
        : [{ heading: t('ai.noChats') }]),
    ];
    menus.openBelow(e.currentTarget as HTMLElement, entries);
  }

  function moreMenu(e: MouseEvent) {
    const current = chat.active;
    if (!current) return;
    menus.openBelow(e.currentTarget as HTMLElement, [
      {
        id: 'rename',
        label: t('ai.rename'),
        onSelect: () =>
          void (async () => {
            const title = await dialogs.prompt({
              title: t('ai.renameTitle'),
              label: t('ai.title'),
              value: current.title,
            });
            if (title?.trim()) await run(() => chat.rename(current.id, title.trim()));
          })(),
      },
      {
        id: 'delete',
        label: t('ai.deleteChat'),
        danger: true,
        onSelect: () =>
          void (async () => {
            const ok = await dialogs.confirm({
              title: t('ai.deleteTitle', { title: current.title || t('ai.untitled') }),
              message: t('ai.deleteMessage'),
              confirmLabel: t('ai.deleteChat'),
              danger: true,
            });
            if (ok) await run(() => chat.remove(current.id));
          })(),
      },
    ]);
  }

  async function pickModel(providerId: string, model: string) {
    await run(async () => {
      if (!chat.active) await chat.newChat();
      await chat.setModel(providerId, model);
    });
  }

  function modelMenu(e: MouseEvent) {
    const entries: MenuEntry[] = [];
    for (const p of ai.providers) {
      entries.push({ heading: p.name });
      for (const m of ai.models[p.id] ?? []) {
        entries.push({
          id: `${p.id}/${m}`,
          label: m,
          checked: chat.active?.providerId === p.id && chat.active?.model === m,
          onSelect: () => void pickModel(p.id, m),
        });
      }
      entries.push({
        id: `${p.id}/other`,
        label: `${t('ai.modelPlaceholder')}…`,
        onSelect: () =>
          void (async () => {
            const model = await dialogs.prompt({
              title: p.name,
              label: t('ai.model'),
              value: chat.active?.providerId === p.id ? (chat.active.model ?? '') : '',
            });
            if (model?.trim()) await pickModel(p.id, model.trim());
          })(),
      });
    }
    menus.openBelow(e.currentTarget as HTMLElement, entries);
  }

  function attach(kind: 'request' | 'response') {
    const id = kind === 'request' ? requestId : responseId;
    if (!id || attachments.some((a) => a.id === id)) return;
    const tab = services.tabs.tabs.find((x) => x.id === requestId);
    const info = tab ? tabInfo(services, tab) : null;
    const label =
      kind === 'request'
        ? `${info?.method ?? ''} ${info?.name ?? ''}`.trim()
        : `${t('shell.response')} ${services.responses.run(requestId!).current?.status ?? ''}`.trim();
    attachments = [...attachments, { kind, id, label }];
  }

  async function send() {
    const message = text.trim();
    if (!message || chat.running) return;
    const refs = attachments.map(({ kind, id }) => ({ kind, id }));
    const before = { text, attachments };
    text = '';
    attachments = [];
    atBottom = true;
    try {
      await services.editor.flushAll();
      await chat.send(message, refs);
    } catch (e) {
      if (!text) {
        text = before.text;
        attachments = before.attachments;
      }
      toasts.error(errorMessage(e));
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void send();
    }
  }

  function confirm(callId: string, decision: AiDecision) {
    void run(() => chat.confirm(callId, decision));
  }

  const callOf = (callId: string | undefined) =>
    chat.messages.flatMap((m) => m.toolCalls).find((c) => c.id === callId);
</script>

<section class="ai" aria-label={t('ai.panel')}>
  <header>
    <button type="button" class="title" onclick={chatsMenu} aria-label={t('ai.chats')}>
      <span>{activeTitle}</span>
      <Icon name="arrow-down" size={12} />
    </button>
    <IconButton
      icon="plus"
      size={14}
      label={t('ai.newChat')}
      onclick={() => void run(() => chat.newChat())}
    />
    {#if chat.active}
      <IconButton icon="more" size={14} label={t('ai.rename')} onclick={moreMenu} />
    {/if}
    {#if onclose}
      <IconButton icon="close" size={14} label={t('ai.hide')} onclick={onclose} />
    {/if}
  </header>

  <div class="messages" bind:this={scroller} {onscroll}>
    {#if !ready}
      <EmptyState title={t('ai.panel')}>
        <p class="hint">{t('ai.setupHint')}</p>
        <Button size="sm" onclick={onsettings}>{t('ai.openSettings')}</Button>
      </EmptyState>
    {:else if chat.messages.length === 0}
      <EmptyState title={t('ai.emptyTitle')}>
        <p class="hint">{t('ai.emptyHint')}</p>
      </EmptyState>
    {:else}
      {#each chat.messages as m (m.id)}
        {#if m.role === 'tool'}
          <ToolCard
            message={m}
            call={callOf(m.toolCallId)}
            waiting={chat.waiting.some((w) => w.callId === m.toolCallId)}
            onconfirm={(d) => confirm(m.toolCallId!, d)}
          />
        {:else if m.role === 'user' || m.content || m.reasoning || m.status !== 'done' || m.error}
          <AiMessage message={m} calling={chat.calling[m.id]} />
        {/if}
      {/each}
    {/if}
  </div>

  {#if ready}
    <footer class="composer">
      {#if attachments.length}
        <div class="attachments">
          {#each attachments as a (a.id)}
            <span class="chip">
              {a.label}
              <IconButton
                icon="close"
                size={10}
                label={t('ai.removeAttachment', { label: a.label })}
                onclick={() => (attachments = attachments.filter((x) => x.id !== a.id))}
              />
            </span>
          {/each}
        </div>
      {/if}
      <textarea
        bind:value={text}
        rows="3"
        placeholder={t('ai.placeholder')}
        aria-label={t('ai.placeholder')}
        title={t('ai.sendHint')}
        {onkeydown}></textarea>
      <div class="bar">
        <IconButton
          icon="file"
          size={14}
          label={t('ai.attachRequest')}
          disabled={!requestId}
          onclick={() => attach('request')}
        />
        <IconButton
          icon="history"
          size={14}
          label={t('ai.attachResponse')}
          disabled={!responseId}
          onclick={() => attach('response')}
        />
        <button type="button" class="model mono" onclick={modelMenu} aria-label={t('ai.model')}>
          <span>{modelLabel}</span>
          <Icon name="arrow-down" size={10} />
        </button>
        <span class="spacer"></span>
        {#if chat.running}
          <Button size="sm" onclick={() => void run(() => chat.cancel())}>
            <Icon name="stop" size={12} />
            {t('ai.stop')}
          </Button>
        {:else}
          <Button size="sm" variant="primary" disabled={!text.trim()} onclick={() => void send()}>
            <Icon name="send" size={12} />
            {t('ai.send')}
          </Button>
        {/if}
      </div>
    </footer>
  {/if}
</section>

<style>
  .ai {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
    background: var(--c-bg);
  }
  header {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px var(--space-2);
    border-bottom: 1px solid var(--c-border);
    min-height: 36px;
  }
  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    font-weight: 600;
    padding: 4px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
  }
  .title span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title:hover {
    background: var(--c-bg-hover, var(--c-bg-sunken));
  }
  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
  }
  .hint {
    color: var(--c-fg-subtle);
    font-size: var(--font-size-sm);
    text-align: center;
    max-width: 320px;
  }
  .composer {
    border-top: 1px solid var(--c-border);
    padding: var(--space-2);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  textarea {
    resize: none;
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--c-border);
    border-radius: var(--radius-sm);
    background: var(--c-bg-input, var(--c-bg));
    color: inherit;
    font: inherit;
    padding: var(--space-2);
    outline: none;
  }
  textarea:focus {
    border-color: var(--c-accent);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .spacer {
    flex: 1;
  }
  .model {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 50%;
    border: 0;
    background: none;
    color: var(--c-fg-muted);
    font-size: var(--font-size-xs);
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .model span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .model:hover {
    color: inherit;
    background: var(--c-bg-sunken);
  }
  .attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-size: var(--font-size-xs);
    padding: 0 0 0 6px;
    border-radius: 999px;
    border: 1px solid var(--c-border);
    color: var(--c-fg-muted);
  }
</style>
