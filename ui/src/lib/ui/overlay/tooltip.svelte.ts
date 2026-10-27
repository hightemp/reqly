// Custom tooltips (`use:tooltip={text}`); one shared bubble rendered by OverlayHost.

const DELAY_MS = 500;

class TooltipState {
  text = $state<string | null>(null);
  anchor = $state<HTMLElement | null>(null);
}

export const tooltipState = new TooltipState();

// Touch screens have no hover: after a tap, focus and emulated mouse events
// must not pop tooltips up. A mouse or the keyboard turns them back on.
let touching = false;
let watching = false;
function watchInput() {
  if (watching || typeof window === 'undefined') return;
  watching = true;
  window.addEventListener('pointerdown', (e) => (touching = e.pointerType === 'touch'), true);
  window.addEventListener('keydown', () => (touching = false), true);
}

export function tooltip(node: HTMLElement, text: string | null | undefined) {
  let current = text;
  let timer: ReturnType<typeof setTimeout> | null = null;

  watchInput();
  const show = () => {
    if (!current || touching) return;
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
