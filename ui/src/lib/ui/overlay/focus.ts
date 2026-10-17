const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"]), [contenteditable="true"]';

export function focusable(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => !el.hasAttribute('inert'),
  );
}

/** Keeps Tab / Shift+Tab inside `root`. Returns true when the key was handled. */
export function trapTab(root: HTMLElement, e: KeyboardEvent): boolean {
  if (e.key !== 'Tab') return false;
  const items = focusable(root);
  if (items.length === 0) return false;
  const first = items[0]!;
  const last = items[items.length - 1]!;
  const active = document.activeElement;
  if (e.shiftKey && (active === first || !root.contains(active))) {
    last.focus();
  } else if (!e.shiftKey && (active === last || !root.contains(active))) {
    first.focus();
  } else {
    return false;
  }
  e.preventDefault();
  return true;
}
