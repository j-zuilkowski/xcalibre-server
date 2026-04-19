import { type ReactNode } from "react";

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

export function Sheet({ open, onOpenChange, children }: SheetProps) {
  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50" role="dialog" aria-modal="true">
      <button
        type="button"
        aria-label="Close panel"
        className="absolute inset-0 bg-black/50"
        onClick={() => onOpenChange(false)}
      />
      {children}
    </div>
  );
}

export function SheetContent({ side = "right", className, children }: SheetContentProps) {
  const sideClasses =
    side === "left"
      ? "left-0 border-r border-zinc-800"
      : "right-0 border-l border-zinc-800";

  return (
    <div
      className={`absolute top-0 h-full w-full max-w-md bg-zinc-950 text-zinc-100 shadow-2xl ${sideClasses} ${
        className ?? ""
      }`.trim()}
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
