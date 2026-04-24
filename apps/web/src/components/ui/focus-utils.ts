const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
  '[contenteditable="true"]',
].join(",");

function isVisible(element: HTMLElement): boolean {
  return element.getClientRects().length > 0;
}

export function getFocusableElements(root: HTMLElement | null): HTMLElement[] {
  if (!root) {
    return [];
  }

  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (element) => !element.hasAttribute("disabled") && isVisible(element),
  );
}

export function focusFirstFocusable(root: HTMLElement | null, fallback?: HTMLElement | null): void {
  const focusable = getFocusableElements(root);
  const target = focusable[0] ?? fallback ?? root;
  target?.focus();
}

export function trapTabKey(event: KeyboardEvent, root: HTMLElement | null): boolean {
  if (event.key !== "Tab") {
    return false;
  }

  const focusable = getFocusableElements(root);
  if (focusable.length === 0) {
    root?.focus();
    event.preventDefault();
    return true;
  }

  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;

  if (event.shiftKey) {
    if (active === first || active === root) {
      last.focus();
      event.preventDefault();
      return true;
    }
    return false;
  }

  if (active === last || active === root) {
    first.focus();
    event.preventDefault();
    return true;
  }

  return false;
}
