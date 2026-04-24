import type { ReactNode } from "react";

type ToastProps = {
  children: ReactNode;
  className?: string;
};

export function Toast({ children, className = "" }: ToastProps) {
  return (
    <div
      role="status"
      aria-live="polite"
      className={`rounded-xl border border-zinc-800 bg-zinc-950 px-4 py-3 text-sm text-zinc-100 shadow-xl ${className}`.trim()}
    >
      {children}
    </div>
  );
}
