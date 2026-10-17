// Custom tooltips (`use:tooltip={text}`); one shared bubble rendered by OverlayHost.

const DELAY_MS = 500;

class TooltipState {
  text = $state<string | null>(null);
  anchor = $state<HTMLElement | null>(null);
}

export const tooltipState = new TooltipState();

export function tooltip(node: HTMLElement, text: string | null | undefined) {
  let current = text;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const show = () => {
    if (!current) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      tooltipState.text = current ?? null;
      tooltipState.anchor = node;
    }, DELAY_MS);
  };
  const hide = () => {
    if (timer) clearTimeout(timer);
    timer = null;
    if (tooltipState.anchor === node) {
      tooltipState.text = null;
      tooltipState.anchor = null;
    }
  };
  const onKeyDown = (e: KeyboardEvent) => e.key === 'Escape' && hide();

  node.addEventListener('mouseenter', show);
  node.addEventListener('mouseleave', hide);
  node.addEventListener('focusin', show);
  node.addEventListener('focusout', hide);
  node.addEventListener('pointerdown', hide);
  node.addEventListener('keydown', onKeyDown);

  return {
    update(next: string | null | undefined) {
      current = next;
      if (tooltipState.anchor === node) tooltipState.text = next ?? null;
    },
    destroy() {
      hide();
      node.removeEventListener('mouseenter', show);
      node.removeEventListener('mouseleave', hide);
      node.removeEventListener('focusin', show);
      node.removeEventListener('focusout', hide);
      node.removeEventListener('pointerdown', hide);
      node.removeEventListener('keydown', onKeyDown);
    },
  };
}
