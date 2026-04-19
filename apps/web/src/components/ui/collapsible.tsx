import {
  type ButtonHTMLAttributes,
  createContext,
  forwardRef,
  type HTMLAttributes,
  type MouseEvent,
  type PropsWithChildren,
  type ReactNode,
  useContext,
  useMemo,
  useState,
} from "react";

type CollapsibleContextValue = {
  open: boolean;
  setOpen: (next: boolean) => void;
};

const CollapsibleContext = createContext<CollapsibleContextValue | null>(null);

type CollapsibleProps = PropsWithChildren<{
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (next: boolean) => void;
  className?: string;
  children: ReactNode;
}>;

function useCollapsibleContext(): CollapsibleContextValue {
  const value = useContext(CollapsibleContext);
  if (!value) {
    throw new Error("Collapsible components must be used inside <Collapsible>.");
  }

  return value;
}

export function Collapsible({
  open,
  defaultOpen = false,
  onOpenChange,
  className,
  children,
}: CollapsibleProps) {
  const [internalOpen, setInternalOpen] = useState(defaultOpen);
  const isControlled = typeof open === "boolean";
  const currentOpen = isControlled ? open : internalOpen;

  const value = useMemo<CollapsibleContextValue>(
    () => ({
      open: currentOpen,
      setOpen: (next: boolean) => {
        if (!isControlled) {
          setInternalOpen(next);
        }
        onOpenChange?.(next);
      },
    }),
    [currentOpen, isControlled, onOpenChange],
  );

  return (
    <CollapsibleContext.Provider value={value}>
      <div data-state={currentOpen ? "open" : "closed"} className={className}>
        {children}
      </div>
    </CollapsibleContext.Provider>
  );
}

type CollapsibleTriggerProps = ButtonHTMLAttributes<HTMLButtonElement>;

export const CollapsibleTrigger = forwardRef<HTMLButtonElement, CollapsibleTriggerProps>(
  function CollapsibleTrigger({ onClick, ...props }, ref) {
    const { open, setOpen } = useCollapsibleContext();

    function handleClick(event: MouseEvent<HTMLButtonElement>) {
      onClick?.(event);
      if (!event.defaultPrevented) {
        setOpen(!open);
      }
    }

    return (
      <button
        type="button"
        {...props}
        ref={ref}
        data-state={open ? "open" : "closed"}
        aria-expanded={open}
        onClick={handleClick}
      />
    );
  },
);

type CollapsibleContentProps = HTMLAttributes<HTMLDivElement>;

export const CollapsibleContent = forwardRef<HTMLDivElement, CollapsibleContentProps>(
  function CollapsibleContent(props, ref) {
    const { open } = useCollapsibleContext();

    if (!open) {
      return null;
    }

    return <div {...props} ref={ref} data-state={open ? "open" : "closed"} />;
  },
);
