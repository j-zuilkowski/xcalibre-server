declare module "react-test-renderer" {
  import type { ReactElement } from "react";

  export namespace TestRenderer {
    interface ReactTestInstance {
      props: Record<string, any>;
      children: Array<TestRenderer.ReactTestInstance | string>;
      find(predicate: (node: ReactTestInstance) => boolean): ReactTestInstance;
      findAll(predicate: (node: ReactTestInstance) => boolean): ReactTestInstance[];
      findByProps(props: Record<string, unknown>): ReactTestInstance;
    }

    interface ReactTestRenderer {
      root: ReactTestInstance;
      unmount(): void;
    }
  }

  const TestRenderer: {
    create(element: ReactElement): TestRenderer.ReactTestRenderer;
  };

  export function act<T>(callback: () => T | Promise<T>): Promise<T> | T;
  export type ReactTestRenderer = TestRenderer.ReactTestRenderer;
  export type ReactTestInstance = TestRenderer.ReactTestInstance;
  export default TestRenderer;
}
