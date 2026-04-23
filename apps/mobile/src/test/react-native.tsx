import React from "react";

type GenericProps = Record<string, unknown> & { children?: React.ReactNode };

function createHost(name: string) {
  const Component = React.forwardRef<unknown, GenericProps>(({ children, ...props }, ref) => {
    return React.createElement(name, { ...props, ref }, children);
  });
  Component.displayName = name;
  return Component;
}

export const View = createHost("View");
export const Text = createHost("Text");
export const ActivityIndicator = createHost("ActivityIndicator");
export const ScrollView = createHost("ScrollView");
export const Modal = createHost("Modal");

export const TextInput = React.forwardRef<unknown, GenericProps>(({ children, ...props }, ref) => {
  return React.createElement("TextInput", { ...props, ref }, children);
});
TextInput.displayName = "TextInput";

export const Pressable = React.forwardRef<unknown, GenericProps>(
  ({ children, ...props }, ref) => {
    const content = typeof children === "function" ? (children as (state: { pressed: boolean }) => React.ReactNode)({ pressed: false }) : children;
    return React.createElement("Pressable", { ...props, ref }, content);
  },
);
Pressable.displayName = "Pressable";

export function FlatList<T>(props: {
  data: T[];
  renderItem: (params: { item: T; index: number }) => React.ReactNode;
  keyExtractor?: (item: T, index: number) => string;
  ListEmptyComponent?: React.ComponentType | React.ReactNode;
  ListFooterComponent?: React.ComponentType | React.ReactNode;
  [key: string]: unknown;
}) {
  const {
    data,
    renderItem,
    keyExtractor,
    ListEmptyComponent,
    ListFooterComponent,
    ...rest
  } = props;

  const empty =
    !data || data.length === 0
      ? typeof ListEmptyComponent === "function"
        ? React.createElement(ListEmptyComponent as React.ComponentType)
        : ListEmptyComponent
      : null;

  const items =
    data && data.length > 0
      ? data.map((item, index) => {
          const key = keyExtractor ? keyExtractor(item, index) : String(index);
          return React.createElement(React.Fragment, { key }, renderItem({ item, index }));
        })
      : null;

  const footer =
    typeof ListFooterComponent === "function"
      ? React.createElement(ListFooterComponent as React.ComponentType)
      : ListFooterComponent;

  return React.createElement("FlatList", rest, items, empty, footer);
}

class AnimatedValue {
  value: number;

  constructor(value: number) {
    this.value = value;
  }

  setValue(value: number) {
    this.value = value;
  }
}

function animationHandle() {
  return {
    start(callback?: () => void) {
      callback?.();
    },
    stop() {
      return undefined;
    },
  };
}

export const Animated = {
  Value: AnimatedValue,
  View: createHost("AnimatedView"),
  timing: () => animationHandle(),
  sequence: () => animationHandle(),
  loop: () => animationHandle(),
};

export const StyleSheet = {
  create(value: Record<string, unknown>) {
    return value;
  },
  flatten<T>(value: T): T {
    return value;
  },
};

export const Platform = {
  OS: "ios",
  select: <T,>(value: { ios?: T; android?: T; default?: T }): T | undefined =>
    value.ios ?? value.default ?? value.android,
};

export const Alert = {
  alert: () => undefined,
};

export function unstable_batchedUpdates<T>(callback: () => T): T {
  return callback();
}
