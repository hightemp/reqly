/** Svelte action: moves the node to `document.body` (escapes overflow/stacking). */
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
