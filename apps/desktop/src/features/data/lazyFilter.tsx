/** CodeMirror loads with the first data view, not at start (task 8.14). */
import { Suspense, lazy } from "react";

import type { FilterEditorProps } from "./FilterEditor";

const FilterEditorModule = lazy(async () => ({
  default: (await import("./FilterEditor")).FilterEditor,
}));

export function LazyFilterEditor(props: FilterEditorProps) {
  return (
    <Suspense fallback={<div className="min-w-0 flex-1" />}>
      <FilterEditorModule {...props} />
    </Suspense>
  );
}
