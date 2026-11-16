import { expect, test } from "@playwright/test";

import { addAndOpenConnection, runInEditor } from "./helpers";

/**
 * p95 frame budget. Locally the grid must hold 50 ms (measured ~23 ms, D-006).
 * Shared CI runners render headless Chromium in software alongside parallel
 * workers, so frame times there measure the runner more than the grid; the
 * CI budget only catches gross regressions.
 */
const P95_BUDGET_MS = process.env.CI ? 120 : 50;
const MAX_RENDERED_CELLS = 1500;

/** Frame times while scrolling a 1000 × 50 result (task 6.1, DECISIONS D-006). */
test("scrolling a 1000 × 50 result keeps frames short", async ({ page }) => {
  await addAndOpenConnection(page, "Wide");
  await runInEditor(page, "SELECT * FROM wide");
  const grid = page.getByRole("grid");
  await expect(grid.getByRole("gridcell").first()).toBeVisible();

  const frames = await grid.evaluate(async (element) => {
    const deltas: number[] = [];
    let last = performance.now();
    const steps = 240;
    for (let i = 0; i < steps; i++) {
      // Down through every row and across every column, then back.
      const progress = i < steps / 2 ? i / (steps / 2) : 2 - i / (steps / 2);
      element.scrollTop = progress * (element.scrollHeight - element.clientHeight);
      element.scrollLeft = progress * (element.scrollWidth - element.clientWidth);
      await new Promise((resolve) => requestAnimationFrame(resolve));
      const now = performance.now();
      deltas.push(now - last);
      last = now;
    }
    return deltas.slice(1).sort((a, b) => a - b);
  });

  const percentile = (p: number) =>
    frames[Math.min(frames.length - 1, Math.floor(frames.length * p))] ?? 0;
  const p50 = percentile(0.5);
  const p95 = percentile(0.95);
  test.info().annotations.push({
    type: "grid scroll frames",
    description: `p50 ${p50.toFixed(1)} ms, p95 ${p95.toFixed(1)} ms, max ${frames.at(-1)?.toFixed(1) ?? "?"} ms`,
  });
  expect(p95).toBeLessThan(P95_BUDGET_MS);
  expect(await grid.getByRole("gridcell").count()).toBeLessThan(MAX_RENDERED_CELLS);
});
