import { serialQueue } from "./serial";

describe("serialQueue", () => {
  it("runs tasks one at a time, in order, even after a failure", async () => {
    const queue = serialQueue();
    const log: string[] = [];
    let release: () => void = () => undefined;
    const first = queue.run(
      () =>
        new Promise<void>((resolve) => {
          log.push("first starts");
          release = () => {
            log.push("first ends");
            resolve();
          };
        }),
    );
    const second = queue.run(() => Promise.reject(new Error("boom")));
    const third = queue.run(() => {
      log.push("third");
      return Promise.resolve(3);
    });

    expect(queue.busy()).toBe(true);
    await Promise.resolve();
    expect(log).toEqual(["first starts"]);
    release();
    await first;
    await expect(second).rejects.toThrow("boom");
    expect(await third).toBe(3);
    expect(log).toEqual(["first starts", "first ends", "third"]);
    expect(queue.busy()).toBe(false);
  });
});
