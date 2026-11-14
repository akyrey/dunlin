import { type ComponentType, type ReactNode, Suspense, lazy } from "react";

/** A component loaded on first render, shown with `fallback` until then (task 8.14). */
export function lazyPanel<P extends object>(
  load: () => Promise<ComponentType<P>>,
  fallback: ReactNode = null,
): ComponentType<P> {
  const Loaded = lazy(async () => ({ default: await load() }));
  function LazyPanel(props: P) {
    return (
      <Suspense fallback={fallback}>
        <Loaded {...props} />
      </Suspense>
    );
  }
  return LazyPanel;
}
