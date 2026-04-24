import { createContext, useContext, useEffect, useRef, type ReactNode } from "react";
import { focusFirstFocusable, trapTabKey } from "./focus-utils";

type SheetProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  children: ReactNode;
};

type SheetContentProps = {
  side?: "left" | "right";
  className?: string;
  children: ReactNode;
};

type SheetContextValue = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

const SheetContext = createContext<SheetContextValue | null>(null);

function useSheetContext(): SheetContextValue {
  const value = useContext(SheetContext);
  if (!value) {
    throw new Error("SheetContent must be rendered inside <Sheet>.");
  }

  return value;
}

export function Sheet({ open, onOpenChange, children }: SheetProps) {
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

  if (!open) {
    return null;
  }

  return (
    <SheetContext.Provider value={{ open, onOpenChange }}>
      <div className="fixed inset-0 z-50" role="dialog" aria-modal="true">
        <div
          className="absolute inset-0 bg-black/50"
          aria-hidden="true"
          onClick={() => onOpenChange(false)}
        />
        {children}
      </div>
    </SheetContext.Provider>
  );
}

export function SheetContent({ side = "right", className, children }: SheetContentProps) {
  const { onOpenChange } = useSheetContext();
  const contentRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    focusFirstFocusable(contentRef.current, contentRef.current);
  }, []);

  useEffect(() => {
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
  }, [onOpenChange]);

  const sideClasses =
    side === "left"
      ? "left-0 border-r border-zinc-800"
      : "right-0 border-l border-zinc-800";

  return (
    <div
      ref={contentRef}
      className={`absolute top-0 h-full w-full max-w-md bg-zinc-950 text-zinc-100 shadow-2xl outline-none ${sideClasses} ${
        className ?? ""
      }`.trim()}
      tabIndex={-1}
    >
      {children}
    </div>
  );
}

export function SheetHeader({ children }: { children: ReactNode }) {
  return <div className="border-b border-zinc-800 px-5 py-4">{children}</div>;
}

export function SheetTitle({ children }: { children: ReactNode }) {
  return <h2 className="text-lg font-semibold text-zinc-100">{children}</h2>;
}
