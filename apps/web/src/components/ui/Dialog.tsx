import { useEffect, useRef, type MutableRefObject, type ReactNode } from "react";
import { focusFirstFocusable, trapTabKey } from "./focus-utils";

type DialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  titleId: string;
  initialFocusRef?: MutableRefObject<HTMLElement | null>;
  children: ReactNode;
};

export function Dialog({ open, onOpenChange, titleId, initialFocusRef, children }: DialogProps) {
  const contentRef = useRef<HTMLDivElement | null>(null);
  const previousActiveElementRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (open) {
      previousActiveElementRef.current = document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
      return;
    }

    previousActiveElementRef.current?.focus();
    previousActiveElementRef.current = null;
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const initialFocus = initialFocusRef?.current ?? null;
    focusFirstFocusable(contentRef.current, initialFocus);
  }, [initialFocusRef, open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onOpenChange(false);
        return;
      }

      trapTabKey(event, contentRef.current);
    }

    const content = contentRef.current;
    content?.addEventListener("keydown", onKeyDown);
    return () => {
      content?.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onOpenChange]);

  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-zinc-950/70 p-4" role="presentation">
      <div
        aria-hidden="true"
        className="absolute inset-0 cursor-default"
        onClick={() => onOpenChange(false)}
      />
      <div
        ref={contentRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="relative z-10 w-full outline-none"
      >
        {children}
      </div>
    </div>
  );
}
