/** CodeMirror and its language packages load on first use, not at start (task 8.14). */
import { type ComponentProps, Suspense, lazy } from "react";

import { Skeleton } from "../../components/Skeleton";

const SqlEditorModule = lazy(async () => ({ default: (await import("./SqlEditor")).SqlEditor }));
const ReadOnlyCodeModule = lazy(async () => ({
  default: (await import("./ReadOnlyCode")).ReadOnlyCode,
}));

export function LazySqlEditor(props: ComponentProps<typeof SqlEditorModule>) {
  return (
    <Suspense fallback={<Skeleton rows={4} label="Loading the editor" />}>
      <SqlEditorModule {...props} />
    </Suspense>
  );
}

export function LazyReadOnlyCode(props: ComponentProps<typeof ReadOnlyCodeModule>) {
  return (
    <Suspense fallback={<Skeleton rows={4} label="Loading" />}>
      <ReadOnlyCodeModule {...props} />
    </Suspense>
  );
}
