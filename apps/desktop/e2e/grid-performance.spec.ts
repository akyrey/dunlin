import { expect, test } from "@playwright/test";

import { addAndOpenConnection, runInEditor } from "./helpers";

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
  expect(p95).toBeLessThan(50);
  expect(await grid.getByRole("gridcell").count()).toBeLessThan(1500);
});
