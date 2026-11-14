/** Runs tasks one after another. A view's connection runs one statement at a time, and the
 * backend refuses a second one while the first runs ("already running in this tab"). */
export type SerialQueue = {
  run: <T>(task: () => Promise<T>) => Promise<T>;
  /** A task is running or waiting. */
  busy: () => boolean;
};

export function serialQueue(): SerialQueue {
  let tail: Promise<unknown> = Promise.resolve();
  let pending = 0;
  return {
    run: (task) => {
      pending++;
      const result = tail.then(task, task).finally(() => {
        pending--;
      });
      // The next task waits for this one however it ends.
      tail = result.catch(() => undefined);
      return result;
    },
    busy: () => pending > 0,
  };
}
